//! Network handling logic for tcplex with bidirectional routing

use crate::config::Config;
use backon::{ExponentialBuilder, Retryable};
use color_eyre::eyre::{Result, WrapErr};
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, broadcast};

/// Create a hex preview of data for logging
fn hex_preview(data: &[u8], max_bytes: usize) -> String {
    let preview_len = data.len().min(max_bytes);
    let hex: String = data[..preview_len]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("");

    if data.len() > max_bytes {
        format!("{hex}... ({} bytes total)", data.len())
    } else {
        format!("{hex} ({} bytes)", data.len())
    }
}

const BUFFER_SIZE: usize = 8192;
const BROADCAST_CHANNEL_SIZE: usize = 100;

/// Connection identifier for routing
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ConnectionId {
    /// Client connection (single connection per client)
    Client(String),
    /// Server connection (multiple connections per server)
    Server { name: String, connection_num: u32 },
}

impl ConnectionId {
    /// Check if this connection belongs to the given target name
    fn matches_target(&self, target_name: &str) -> bool {
        match self {
            ConnectionId::Client(name) => name == target_name,
            ConnectionId::Server { name, .. } => name == target_name,
        }
    }

    /// Get the base name (without connection number)
    fn base_name(&self) -> &str {
        match self {
            ConnectionId::Client(name) => name,
            ConnectionId::Server { name, .. } => name,
        }
    }
}

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionId::Client(name) => write!(f, "{name}"),
            ConnectionId::Server {
                name,
                connection_num,
            } => write!(f, "{name}-{connection_num}"),
        }
    }
}

/// Node represents either a server or client connection
#[derive(Debug)]
struct Node {
    sender: broadcast::Sender<Vec<u8>>,
}

/// The multiplexer router that manages all connections
pub struct Router {
    config: Config,
    nodes: Arc<RwLock<HashMap<ConnectionId, Arc<Node>>>>,
}

impl Router {
    /// Create a new router with the given configuration
    pub fn new(config: Config) -> Self {
        Self {
            config,
            nodes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start the router - spawn all servers and clients
    pub async fn run(self) -> Result<()> {
        let router = Arc::new(self);
        let mut handles = vec![];

        // Start all servers first
        for (name, server_config) in &router.config.server {
            let router_clone = Arc::clone(&router);
            let name = name.clone();
            let port = server_config.port;

            let handle = tokio::spawn(async move {
                if let Err(e) = router_clone.run_server(&name, port).await {
                    error!("Server '{name}' error: {e:#}");
                }
            });
            handles.push(handle);
        }

        // Give servers time to start listening before clients connect
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Start all clients
        for (name, client_config) in &router.config.client {
            let router_clone = Arc::clone(&router);
            let name = name.clone();
            let addr = client_config.addr;

            let handle = tokio::spawn(async move {
                if let Err(e) = router_clone.run_client(&name, addr).await {
                    error!("Client '{name}' error: {e:#}");
                }
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            let _ = handle.await;
        }

        Ok(())
    }

    /// Run a server node - accepts multiple clients
    async fn run_server(self: Arc<Self>, name: &str, port: u16) -> Result<()> {
        let bind_addr = format!("0.0.0.0:{port}");
        let listener = TcpListener::bind(&bind_addr)
            .await
            .wrap_err_with(|| format!("Failed to bind server '{name}' to {bind_addr}"))?;

        info!("[{name}] Listening on {bind_addr}");

        let mut connection_num = 0u32;

        // Accept multiple connections in a loop
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    connection_num += 1;
                    let conn_id = ConnectionId::Server {
                        name: name.to_string(),
                        connection_num,
                    };
                    info!("[{name}] Accepted connection #{connection_num} from {addr}");

                    // Spawn a task for this connection
                    let router_clone = Arc::clone(&self);
                    let name_for_handler = name.to_string();
                    tokio::spawn(async move {
                        if let Err(e) = router_clone
                            .handle_node_connection(conn_id.clone(), stream)
                            .await
                        {
                            error!(
                                "[{name_for_handler}] Connection #{connection_num} error: {e:#}"
                            );
                        }
                        info!("[{name_for_handler}] Connection #{connection_num} closed");
                    });
                }
                Err(e) => {
                    error!("[{name}] Failed to accept connection: {e:#}");
                }
            }
        }
    }

    /// Run a client node with automatic reconnection
    ///
    /// When a connection is rebuilt after dropping, the client:
    /// 1. Unregisters the old connection from the routing table
    /// 2. Establishes a new connection with exponential backoff
    /// 3. Registers the new connection, which will receive data forwarded from other nodes
    async fn run_client(self: Arc<Self>, name: &str, addr: SocketAddr) -> Result<()> {
        let backoff = ExponentialBuilder::default()
            .with_min_delay(std::time::Duration::from_secs(1))
            .with_max_delay(std::time::Duration::from_secs(30))
            .with_max_times(usize::MAX); // Retry indefinitely

        loop {
            let connect_result = (|| async {
                info!("[{name}] Connecting to {addr}...");
                TcpStream::connect(addr).await
            })
            .retry(backoff)
            .notify(|err, dur| {
                warn!("[{name}] Failed to connect to {addr}: {err}");
                warn!("[{name}] Will retry in {dur:?}...");
            })
            .await;

            match connect_result {
                Ok(stream) => {
                    info!("[{name}] Connected to {addr}");

                    // Register this node and handle its connection
                    let conn_id = ConnectionId::Client(name.to_string());
                    let router_clone = Arc::clone(&self);
                    if let Err(e) = router_clone.handle_node_connection(conn_id, stream).await {
                        error!("[{name}] Connection error: {e:#}");
                    }

                    // Connection closed, will reconnect
                    info!("[{name}] Disconnected, reconnecting...");
                }
                Err(e) => {
                    error!("[{name}] Exhausted all retries: {e:#}");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a connection for a node (server or client)
    async fn handle_node_connection(
        self: Arc<Self>,
        conn_id: ConnectionId,
        stream: TcpStream,
    ) -> Result<()> {
        let peer_addr = stream
            .peer_addr()
            .wrap_err_with(|| format!("Failed to get peer address for '{conn_id}'"))?;

        // Create broadcast channel for incoming data to this connection
        let (incoming_tx, _) = broadcast::channel::<Vec<u8>>(BROADCAST_CHANNEL_SIZE);

        let node = Arc::new(Node {
            sender: incoming_tx.clone(),
        });

        // Register the connection
        {
            let mut nodes = self.nodes.write().await;
            nodes.insert(conn_id.clone(), Arc::clone(&node));
            debug!(
                "Registered connection '{conn_id}' (total connections: {})",
                nodes.len()
            );
        }

        // Get target names for this node
        let base_name = conn_id.base_name();
        let targets = self.get_targets(base_name);
        debug!("Connection '{conn_id}' will forward to targets: {targets:?}");

        // Create channel for outgoing data from this connection
        let (outgoing_tx, mut rx_outgoing) = broadcast::channel::<Vec<u8>>(BROADCAST_CHANNEL_SIZE);

        // Spawn task to forward data to target nodes
        let nodes_clone = Arc::clone(&self.nodes);
        let base_name_for_forward = base_name.to_string();
        let targets_for_forward = targets.clone();

        tokio::spawn(async move {
            if let Err(e) = forward_to_targets(
                &nodes_clone,
                &base_name_for_forward,
                &targets_for_forward,
                &mut rx_outgoing,
            )
            .await
            {
                error!("[{base_name_for_forward}] Error forwarding to targets: {e:#}");
            }
        });

        // Split stream for bidirectional I/O
        let (mut read_half, mut write_half) = stream.into_split();

        // Spawn task to write data received from other nodes to TCP
        let base_name_for_write = base_name.to_string();
        let conn_id_display = conn_id.to_string();
        let mut rx_incoming = incoming_tx.subscribe();
        let write_handle = tokio::spawn(async move {
            while let Ok(data) = rx_incoming.recv().await {
                trace!(
                    "[{base_name_for_write}] Writing {len} bytes to TCP",
                    len = data.len()
                );
                if let Err(e) = write_half.write_all(&data).await {
                    error!("[{base_name_for_write}] Error writing to {conn_id_display}: {e:#}");
                    break;
                }
            }
        });

        // Read from TCP connection and broadcast to outgoing channel
        let base_name_for_read = base_name.to_string();
        let conn_id_for_unregister = conn_id.clone();
        let mut buffer = [0u8; BUFFER_SIZE];
        loop {
            match read_half.read(&mut buffer).await {
                Ok(0) => {
                    info!("[{base_name_for_read}] Connection from {peer_addr} closed");
                    break;
                }
                Ok(n) => {
                    let data = buffer[..n].to_vec();

                    // Log packet at different verbosity levels
                    debug!("[{base_name_for_read}] Received {n} bytes from {peer_addr}");
                    trace!(
                        "[{base_name_for_read} <- {peer_addr}] Packet: {}",
                        hex_preview(&data, 32)
                    );

                    // Log routing information
                    if !targets.is_empty() {
                        trace!(
                            "[{base_name_for_read} -> {}] Routing packet",
                            targets.join(", ")
                        );
                    }

                    // Broadcast to outgoing channel for forwarding
                    if outgoing_tx.send(data).is_err() {
                        warn!("[{base_name_for_read}] No active receivers");
                    }
                }
                Err(e) => {
                    error!("[{base_name_for_read}] Error reading from {peer_addr}: {e:#}");
                    break;
                }
            }
        }

        // Unregister the connection
        {
            let mut nodes = self.nodes.write().await;
            nodes.remove(&conn_id_for_unregister);
            debug!("[{base_name_for_read}] Unregistered connection {conn_id_for_unregister}");
        }

        // Clean up write task
        write_handle.abort();

        Ok(())
    }

    /// Get the target names for a given node
    fn get_targets(&self, name: &str) -> Vec<String> {
        if let Some(server) = self.config.server.get(name) {
            return server.target.clone();
        }
        if let Some(client) = self.config.client.get(name) {
            return client.target.clone();
        }
        vec![]
    }
}

/// Forward data from a source node to its target nodes
/// For servers with multiple connections, this broadcasts to ALL connections
async fn forward_to_targets(
    nodes: &Arc<RwLock<HashMap<ConnectionId, Arc<Node>>>>,
    source_name: &str,
    targets: &[String],
    rx: &mut broadcast::Receiver<Vec<u8>>,
) -> Result<()> {
    info!("[{source_name}] Will forward to targets: {targets:?}");

    // Receive data and forward to targets
    // Note: Forwards immediately without waiting for all targets to be ready
    // This allows the system to continue working even if some nodes are disconnected
    while let Ok(data) = rx.recv().await {
        let nodes_read = nodes.read().await;
        let data_len = data.len();

        for target_name in targets {
            // Find all connections for this target (handles both clients and servers)
            let mut found_any = false;

            for (conn_id, target_node) in nodes_read.iter() {
                // Check if this connection belongs to the target
                if conn_id.matches_target(target_name) {
                    found_any = true;
                    match target_node.sender.send(data.clone()) {
                        Ok(_) => {
                            debug!("[{source_name} -> {target_name}] Forwarded {data_len} bytes");
                            trace!(
                                "[{source_name} -> {target_name}] Data: {}",
                                hex_preview(&data, 32)
                            );
                        }
                        Err(e) => {
                            warn!("[{source_name}] Failed to send to '{target_name}': {e}");
                        }
                    }
                }
            }

            if !found_any {
                debug!("[{source_name}] Target '{target_name}' not yet connected or disconnected");
            }
        }
    }

    Ok(())
}
