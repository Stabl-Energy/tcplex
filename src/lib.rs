//! tcplex - Bidirectional TCP multiplexer with config-based routing

pub mod config;
pub mod network;

pub use config::Config;
pub use network::Router;
