# tcplex

A multi-directional TCP multiplexer with config-based routing. Forward TCP traffic between multiple servers and clients using a declarative TOML configuration file. Built with Rust and Tokio for reliability and ease of use.

## Features

- **Bidirectional Routing**: Define servers and clients that can forward data to each other
- **Config-Based**: Use TOML configuration files for declarative routing rules
- **Concurrent Connections**: Handle multiple connections simultaneously

## Installation

### Build from Source

```bash
git clone <repository-url>
cd multiplexer
cargo build --release
```

The compiled binary will be available at `target/release/tcplex`.

> **Note**: If you have [just](https://github.com/casey/just) installed, you can use `just build-release` instead of `cargo build --release`. See the [Development](#development) section for more commands.

## Usage

### Basic Usage

Create a `tcplex.conf` file with your routing configuration:

```toml
[server.gateway]
port = 8000
target = ["backend1", "backend2"]

[client.backend1]
ip = "192.168.1.10"
port = 8080
target = ["gateway"]

[client.backend2]
ip = "192.168.1.20"
port = 8080
target = ["gateway"]
```

Then run tcplex:

```bash
tcplex
# or specify a custom config file
tcplex -c /path/to/config.toml
```

### Command-Line Options

```
tcplex - Bidirectional TCP multiplexer with config-based routing

Usage: tcplex [OPTIONS]

Options:
  -c, --config <CONFIG>
          Path to the configuration file
          [default: tcplex.conf]

  -v, --verbose...
          Increase logging verbosity

  -q, --quiet...
          Decrease logging verbosity

  -h, --help
          Print help

  -V, --version
          Print version
```

### Configuration Format

#### Server Definition

Servers listen for incoming connections on a specified port:

```toml
[server.name]
port = 8000              # Port to listen on
target = ["client1"]     # List of nodes to forward data to
```

#### Client Definition

Clients connect to remote servers:

```toml
[client.name]
ip = "10.0.0.1"          # IP address to connect to
port = 8000              # Port to connect to
target = ["server1"]     # List of nodes to forward data to
```

### Logging

Enable verbose logging to see connection details:

```bash
# Info level logging
tcplex -v

# Debug level logging
tcplex -vv

# Trace level logging
tcplex -vvv
```

## How It Works

1. **Configuration Loading**: tcplex reads the TOML configuration file and validates all node references
2. **Node Startup**: All servers start listening and all clients attempt to connect
3. **Resilient Forwarding**: Nodes immediately start forwarding to available targets - no waiting required
4. **Bidirectional Routing**: Data flows according to the routing rules, even with partial connectivity
5. **Automatic Routing**: Data received by any node is automatically forwarded to its configured targets
6. **Graceful Degradation**: When nodes disconnect, other connections continue working normally

### Architecture Example

```
┌──────────┐         ┌─────────┐
│ Client A │◄───────►│ Server  │
└──────────┘         │ "serv"  │
                     │ :8000   │
┌──────────┐         │         │
│ Client B │◄───────►│         │
└──────────┘         └─────────┘
```

In this setup:
- Server "serv" listens on port 8000 and forwards data to both Client A and Client B
- Client A connects to its remote server and forwards data back to "serv"
- Client B connects to its remote server and forwards data back to "serv"
- Data flows bidirectionally between all connected nodes


## Configuration Examples

### Example 1: Simple Relay

Forward traffic between two endpoints:

```toml
[server.relay]
port = 9000
target = ["backend"]

[client.backend]
ip = "192.168.1.100"
port = 8080
target = ["relay"]
```

### Example 2: Multi-Way Mirror

Mirror traffic to multiple destinations:

```toml
[server.main]
port = 8000
target = ["prod", "staging", "dev"]

[client.prod]
ip = "prod.example.com"
port = 8080
target = []

[client.staging]
ip = "staging.example.com"
port = 8080
target = []

[client.dev]
ip = "dev.example.com"
port = 8080
target = []
```

### Example 3: Bidirectional Sync

Synchronize data between two servers:

```toml
[server.siteA]
port = 8000
target = ["siteB"]

[client.siteB]
ip = "site-b.example.com"
port = 8000
target = ["siteA"]
```

## Development

### Just Commands

This project uses [just](https://github.com/casey/just) as a command runner for common development tasks. Run `just` to see all available commands:

```bash
just                  # Show all available commands
just build            # Build in debug mode
just build-release    # Build in release mode
just test             # Run all tests
just lint             # Run clippy linter
just fmt              # Format code
just check            # Check code without building
just ci               # Run all checks (format, lint, test)
```

## Troubleshooting

### Connection Refused

If clients show "connection refused":
- Ensure the target servers are running and reachable
- Verify IP addresses and ports in the configuration
- Check firewall rules and network connectivity

### No Data Forwarding

If nodes connect but data isn't forwarded:
- Enable verbose logging (`-vv` or `-vvv`) to see routing details
- Check that target references in the config are correct
- Look for debug messages indicating disconnected targets: `"Target node 'X' not yet connected or disconnected"`

### Invalid Configuration

If tcplex fails to start:
- Verify TOML syntax is correct
- Ensure all target node names exist in the configuration
- Check that port numbers are valid (1-65535)

### Connection Failures

When a connection breaks:
- **Resilient operation**: Other healthy connections continue routing normally
- **Message handling**: Data destined for disconnected nodes is logged and dropped
- **Client automatic reconnection**: Clients automatically reconnect with exponential backoff (1s to 30s)
- **Server behavior**: Servers accept **multiple connections** simultaneously and broadcast to all connected clients

### Server Multiplexing

Servers automatically multiplex data to all connected clients:
- A server can accept unlimited concurrent connections
- Data routed to a server is broadcast to **all** its connected clients
- Each client connection is independent - disconnection of one doesn't affect others
- Perfect for scenarios where multiple clients need to receive the same data stream

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit issues or pull requests.
