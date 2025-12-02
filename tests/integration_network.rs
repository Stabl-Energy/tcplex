//! Integration tests for network routing functionality
//!
//! These tests verify the actual TCP multiplexing behavior:
//! - Bidirectional routing between nodes
//! - Hub-and-spoke routing patterns
//! - Server-to-client routing
//! - Multiple sequential packets
//! - CED scenario (multiple servers, one client master)
//! - CED config file validation (examples/ced.conf)
//! - Connection failure resilience
//! - Client automatic reconnection
//! - Server with multiple client connections (broadcast to all)
//! - Sink nodes (no outgoing targets)

mod common;

use std::time::Duration;
use tcplex::{Config, Router};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

/// Test bidirectional communication between two nodes
#[tokio::test]
async fn test_bidirectional_routing() {
    let config_content = r#"
[server.nodeA]
port = 19001
target = ["nodeB"]

[server.nodeB]
port = 19002
target = ["nodeA"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();
    let router = Router::new(config);

    // Start router in background
    tokio::spawn(async move {
        let _ = router.run().await;
    });

    // Give router time to start listening
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect to both servers
    let mut client_a = tokio::net::TcpStream::connect("127.0.0.1:19001")
        .await
        .expect("Failed to connect to nodeA");
    let mut client_b = tokio::net::TcpStream::connect("127.0.0.1:19002")
        .await
        .expect("Failed to connect to nodeB");

    // Give time for connections to establish
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send data from A to B
    client_a
        .write_all(b"Hello from A")
        .await
        .expect("Failed to write to A");

    // Read from B
    let mut buf = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(2), client_b.read(&mut buf))
        .await
        .expect("Timeout reading from B")
        .expect("Failed to read from B");

    assert_eq!(&buf[..n], b"Hello from A");

    // Send data from B to A
    client_b
        .write_all(b"Hello from B")
        .await
        .expect("Failed to write to B");

    // Read from A
    let n = timeout(Duration::from_secs(2), client_a.read(&mut buf))
        .await
        .expect("Timeout reading from A")
        .expect("Failed to read from A");

    assert_eq!(&buf[..n], b"Hello from B");
}

/// Test hub-and-spoke routing pattern
#[tokio::test]
async fn test_hub_spoke_routing() {
    let config_content = r#"
[server.hub]
port = 19010
target = ["spoke1", "spoke2"]

[server.spoke1]
port = 19011
target = ["hub"]

[server.spoke2]
port = 19012
target = ["hub"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();
    let router = Router::new(config);

    // Start router in background
    tokio::spawn(async move {
        let _ = router.run().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect to all nodes
    let mut hub = tokio::net::TcpStream::connect("127.0.0.1:19010")
        .await
        .expect("Failed to connect to hub");
    let mut spoke1 = tokio::net::TcpStream::connect("127.0.0.1:19011")
        .await
        .expect("Failed to connect to spoke1");
    let mut spoke2 = tokio::net::TcpStream::connect("127.0.0.1:19012")
        .await
        .expect("Failed to connect to spoke2");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send from hub, should reach both spokes
    hub.write_all(b"Broadcast")
        .await
        .expect("Failed to write to hub");

    let mut buf = vec![0u8; 1024];

    // Read from spoke1
    let n = timeout(Duration::from_secs(2), spoke1.read(&mut buf))
        .await
        .expect("Timeout reading from spoke1")
        .expect("Failed to read from spoke1");
    assert_eq!(&buf[..n], b"Broadcast");

    // Read from spoke2
    let n = timeout(Duration::from_secs(2), spoke2.read(&mut buf))
        .await
        .expect("Timeout reading from spoke2")
        .expect("Failed to read from spoke2");
    assert_eq!(&buf[..n], b"Broadcast");
}

/// Test server-to-client routing
#[tokio::test]
async fn test_server_client_routing() {
    let config_content = r#"
[server.server1]
port = 19020
target = ["client1"]

[client.client1]
addr = "127.0.0.1:19021"
target = ["server1"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();
    let router = Router::new(config);

    // Start a listener for the client to connect to
    let listener = TcpListener::bind("127.0.0.1:19021")
        .await
        .expect("Failed to bind listener");

    tokio::spawn(async move {
        let _ = router.run().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect to server1
    let mut server_conn = tokio::net::TcpStream::connect("127.0.0.1:19020")
        .await
        .expect("Failed to connect to server1");

    // Accept the client connection
    let (mut client_conn, _) = timeout(Duration::from_secs(2), listener.accept())
        .await
        .expect("Timeout waiting for client connection")
        .expect("Failed to accept client connection");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send from server to client
    server_conn
        .write_all(b"Server to Client")
        .await
        .expect("Failed to write to server");

    let mut buf = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(2), client_conn.read(&mut buf))
        .await
        .expect("Timeout reading from client")
        .expect("Failed to read from client");

    assert_eq!(&buf[..n], b"Server to Client");

    // Send from client to server
    client_conn
        .write_all(b"Client to Server")
        .await
        .expect("Failed to write to client");

    let n = timeout(Duration::from_secs(2), server_conn.read(&mut buf))
        .await
        .expect("Timeout reading from server")
        .expect("Failed to read from server");

    assert_eq!(&buf[..n], b"Client to Server");
}

/// Test multiple writes and reads
#[tokio::test]
async fn test_multiple_packets() {
    let config_content = r#"
[server.nodeA]
port = 19030
target = ["nodeB"]

[server.nodeB]
port = 19031
target = ["nodeA"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();
    let router = Router::new(config);

    tokio::spawn(async move {
        let _ = router.run().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client_a = tokio::net::TcpStream::connect("127.0.0.1:19030")
        .await
        .expect("Failed to connect to nodeA");
    let mut client_b = tokio::net::TcpStream::connect("127.0.0.1:19031")
        .await
        .expect("Failed to connect to nodeB");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send multiple packets from A to B
    for i in 0..5 {
        let msg = format!("Packet {i}");
        client_a
            .write_all(msg.as_bytes())
            .await
            .expect("Failed to write to A");

        let mut buf = vec![0u8; 1024];
        let n = timeout(Duration::from_secs(2), client_b.read(&mut buf))
            .await
            .expect("Timeout reading from B")
            .expect("Failed to read from B");

        assert_eq!(&buf[..n], msg.as_bytes());
    }
}

/// Test CED scenario: two CED servers with one master client
#[tokio::test]
async fn test_ced_master_routing() {
    let config_content = r#"
[server.cedA]
port = 19050
target = ["master"]

[server.cedB]
port = 19051
target = ["master"]

[client.master]
addr = "127.0.0.1:19052"
target = ["cedA", "cedB"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();
    let router = Router::new(config);

    // Start a listener for the master client to connect to
    let listener = TcpListener::bind("127.0.0.1:19052")
        .await
        .expect("Failed to bind master listener");

    tokio::spawn(async move {
        let _ = router.run().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect to both CED servers
    let mut ced_a = tokio::net::TcpStream::connect("127.0.0.1:19050")
        .await
        .expect("Failed to connect to cedA");
    let mut ced_b = tokio::net::TcpStream::connect("127.0.0.1:19051")
        .await
        .expect("Failed to connect to cedB");

    // Accept the master client connection
    let (mut master, _) = timeout(Duration::from_secs(2), listener.accept())
        .await
        .expect("Timeout waiting for master connection")
        .expect("Failed to accept master connection");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Test 1: Send from cedA to master
    ced_a
        .write_all(b"Data from CED A")
        .await
        .expect("Failed to write to cedA");

    let mut buf = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(2), master.read(&mut buf))
        .await
        .expect("Timeout reading from master")
        .expect("Failed to read from master");
    assert_eq!(&buf[..n], b"Data from CED A");

    // Test 2: Send from cedB to master
    ced_b
        .write_all(b"Data from CED B")
        .await
        .expect("Failed to write to cedB");

    let n = timeout(Duration::from_secs(2), master.read(&mut buf))
        .await
        .expect("Timeout reading from master")
        .expect("Failed to read from master");
    assert_eq!(&buf[..n], b"Data from CED B");

    // Test 3: Send from master, should reach both CEDs
    master
        .write_all(b"Command from Master")
        .await
        .expect("Failed to write to master");

    // Read from cedA
    let n = timeout(Duration::from_secs(2), ced_a.read(&mut buf))
        .await
        .expect("Timeout reading from cedA")
        .expect("Failed to read from cedA");
    assert_eq!(&buf[..n], b"Command from Master");

    // Read from cedB
    let n = timeout(Duration::from_secs(2), ced_b.read(&mut buf))
        .await
        .expect("Timeout reading from cedB")
        .expect("Failed to read from cedB");
    assert_eq!(&buf[..n], b"Command from Master");

    // Test 4: Multiple sequential messages from different CEDs
    ced_a.write_all(b"A1").await.expect("Failed to write A1");
    let n = timeout(Duration::from_secs(2), master.read(&mut buf))
        .await
        .expect("Timeout reading A1")
        .expect("Failed to read A1");
    assert_eq!(&buf[..n], b"A1");

    ced_b.write_all(b"B1").await.expect("Failed to write B1");
    let n = timeout(Duration::from_secs(2), master.read(&mut buf))
        .await
        .expect("Timeout reading B1")
        .expect("Failed to read B1");
    assert_eq!(&buf[..n], b"B1");

    ced_a.write_all(b"A2").await.expect("Failed to write A2");
    let n = timeout(Duration::from_secs(2), master.read(&mut buf))
        .await
        .expect("Timeout reading A2")
        .expect("Failed to read A2");
    assert_eq!(&buf[..n], b"A2");
}

/// Test CED config file from examples directory
#[tokio::test]
async fn test_ced_config_file() {
    // Load the actual CED config file
    let config_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("ced.toml");

    let config = Config::from_file(&config_path).expect("Failed to load examples/ced.toml");

    // Verify the config structure
    assert!(config.server.contains_key("cedA"));
    assert!(config.server.contains_key("cedB"));
    assert!(config.client.contains_key("master"));

    let ced_a = config.server.get("cedA").unwrap();
    assert_eq!(ced_a.port, 8001);
    assert_eq!(ced_a.target, vec!["master"]);

    let ced_b = config.server.get("cedB").unwrap();
    assert_eq!(ced_b.port, 8002);
    assert_eq!(ced_b.target, vec!["master"]);

    let master = config.client.get("master").unwrap();
    assert_eq!(master.addr.to_string(), "10.20.0.50:64646");
    assert_eq!(master.target, vec!["cedA", "cedB"]);
}

/// Test connection failure behavior - messages are dropped if target disconnected
#[tokio::test]
async fn test_connection_failure() {
    let config_content = r#"
[server.nodeA]
port = 19060
target = ["nodeB"]

[server.nodeB]
port = 19061
target = ["nodeA"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();
    let router = Router::new(config);

    tokio::spawn(async move {
        let _ = router.run().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect to both nodes
    let mut client_a = tokio::net::TcpStream::connect("127.0.0.1:19060")
        .await
        .expect("Failed to connect to nodeA");
    let mut client_b = tokio::net::TcpStream::connect("127.0.0.1:19061")
        .await
        .expect("Failed to connect to nodeB");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify communication works
    client_a
        .write_all(b"Test message")
        .await
        .expect("Failed to write");

    let mut buf = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(2), client_b.read(&mut buf))
        .await
        .expect("Timeout")
        .expect("Failed to read");
    assert_eq!(&buf[..n], b"Test message");

    // Drop client_b connection (simulating network failure)
    drop(client_b);

    // Give time for router to detect disconnection
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Try to send from A - should succeed (write succeeds)
    // but data won't reach B (it's disconnected and will be logged as debug message)
    client_a
        .write_all(b"Lost message")
        .await
        .expect("Write should succeed");

    // Wait a bit to ensure message was processed
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Note: Without the wait_for_targets blocking, the system continues to work
    // Messages to disconnected nodes are simply logged and dropped
    // This is the desired behavior for resilient operation
}

/// Test client automatic reconnection
#[tokio::test]
async fn test_client_reconnection() {
    let config_content = r#"
[server.server]
port = 19070
target = ["client"]

[client.client]
addr = "127.0.0.1:19071"
target = ["server"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();
    let router = Router::new(config);

    tokio::spawn(async move {
        let _ = router.run().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Start a listener for the client to connect to
    let listener = TcpListener::bind("127.0.0.1:19071")
        .await
        .expect("Failed to bind listener");

    // Accept first connection
    let mut server_conn = tokio::net::TcpStream::connect("127.0.0.1:19070")
        .await
        .expect("Failed to connect to server");

    let (mut client_conn_1, _) = timeout(Duration::from_secs(2), listener.accept())
        .await
        .expect("Timeout waiting for client connection")
        .expect("Failed to accept first client connection");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Test communication works
    server_conn.write_all(b"Test1").await.expect("Write failed");
    let mut buf = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(2), client_conn_1.read(&mut buf))
        .await
        .expect("Timeout")
        .expect("Read failed");
    assert_eq!(&buf[..n], b"Test1");

    // Drop the client connection to simulate failure
    drop(client_conn_1);

    // Give time for disconnection to be detected
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Client should automatically reconnect - accept the new connection
    let (mut client_conn_2, _) = timeout(Duration::from_secs(3), listener.accept())
        .await
        .expect("Timeout waiting for client reconnection")
        .expect("Failed to accept reconnected client");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Test communication works after reconnection
    server_conn
        .write_all(b"Test2")
        .await
        .expect("Write failed after reconnect");
    let n = timeout(Duration::from_secs(2), client_conn_2.read(&mut buf))
        .await
        .expect("Timeout after reconnect")
        .expect("Read failed after reconnect");
    assert_eq!(&buf[..n], b"Test2");
}

/// Test server with multiple client connections - multiplexes to all
#[tokio::test]
async fn test_server_multiple_clients() {
    let config_content = r#"
[server.hub]
port = 19080
target = ["client"]

[client.client]
addr = "127.0.0.1:19081"
target = ["hub"]
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();
    let router = Router::new(config);

    tokio::spawn(async move {
        let _ = router.run().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Start listener for client
    let listener = TcpListener::bind("127.0.0.1:19081")
        .await
        .expect("Failed to bind listener");

    // Connect THREE clients to the hub server
    let mut hub_client1 = tokio::net::TcpStream::connect("127.0.0.1:19080")
        .await
        .expect("Failed to connect client 1");
    let mut hub_client2 = tokio::net::TcpStream::connect("127.0.0.1:19080")
        .await
        .expect("Failed to connect client 2");
    let mut hub_client3 = tokio::net::TcpStream::connect("127.0.0.1:19080")
        .await
        .expect("Failed to connect client 3");

    // Accept client connection
    let (mut client_conn, _) = timeout(Duration::from_secs(2), listener.accept())
        .await
        .expect("Timeout waiting for client connection")
        .expect("Failed to accept client connection");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send from client - should reach ALL three hub connections
    client_conn
        .write_all(b"Broadcast to all")
        .await
        .expect("Failed to write from client");

    let mut buf = vec![0u8; 1024];

    // All three hub clients should receive the message
    let n1 = timeout(Duration::from_secs(2), hub_client1.read(&mut buf))
        .await
        .expect("Timeout reading client 1")
        .expect("Failed to read client 1");
    assert_eq!(&buf[..n1], b"Broadcast to all");

    let n2 = timeout(Duration::from_secs(2), hub_client2.read(&mut buf))
        .await
        .expect("Timeout reading client 2")
        .expect("Failed to read client 2");
    assert_eq!(&buf[..n2], b"Broadcast to all");

    let n3 = timeout(Duration::from_secs(2), hub_client3.read(&mut buf))
        .await
        .expect("Timeout reading client 3")
        .expect("Failed to read client 3");
    assert_eq!(&buf[..n3], b"Broadcast to all");

    // Send from one hub client - should only reach the client (not other hub clients)
    hub_client1
        .write_all(b"From hub1")
        .await
        .expect("Failed to write from hub1");

    let n = timeout(Duration::from_secs(2), client_conn.read(&mut buf))
        .await
        .expect("Timeout reading from client")
        .expect("Failed to read from client");
    assert_eq!(&buf[..n], b"From hub1");

    // Verify hub_client2 and hub_client3 did NOT receive the message
    // (timeout expected - they shouldn't get data)
    let result2 = timeout(Duration::from_millis(300), hub_client2.read(&mut buf)).await;
    assert!(
        result2.is_err(),
        "hub_client2 should not receive data from hub_client1"
    );

    let result3 = timeout(Duration::from_millis(300), hub_client3.read(&mut buf)).await;
    assert!(
        result3.is_err(),
        "hub_client3 should not receive data from hub_client1"
    );
}

/// Test no-target node (sink)
#[tokio::test]
async fn test_sink_node() {
    let config_content = r#"
[server.source]
port = 19040
target = ["sink"]

[server.sink]
port = 19041
target = []
"#;

    let config_file = common::create_test_config(config_content);
    let config = Config::from_file(config_file.path()).unwrap();
    let router = Router::new(config);

    tokio::spawn(async move {
        let _ = router.run().await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut source = tokio::net::TcpStream::connect("127.0.0.1:19040")
        .await
        .expect("Failed to connect to source");
    let mut sink = tokio::net::TcpStream::connect("127.0.0.1:19041")
        .await
        .expect("Failed to connect to sink");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send from source
    source
        .write_all(b"To sink")
        .await
        .expect("Failed to write to source");

    // Sink should receive data
    let mut buf = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(2), sink.read(&mut buf))
        .await
        .expect("Timeout reading from sink")
        .expect("Failed to read from sink");

    assert_eq!(&buf[..n], b"To sink");

    // Send from sink - should go nowhere but not error
    sink.write_all(b"From sink")
        .await
        .expect("Failed to write to sink");

    // Source should NOT receive anything (timeout expected)
    let result = timeout(Duration::from_millis(500), source.read(&mut buf)).await;
    assert!(result.is_err(), "Should timeout - no data expected");
}
