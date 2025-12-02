//! Configuration file parsing for tcplex

use color_eyre::eyre::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;

/// Root configuration structure
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// Server configurations
    #[serde(default)]
    pub server: HashMap<String, ServerConfig>,

    /// Client configurations
    #[serde(default)]
    pub client: HashMap<String, ClientConfig>,
}

/// Server configuration - listens for incoming connections
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// Port to listen on
    pub port: u16,

    /// Target clients/servers to forward data to
    pub target: Vec<String>,
}

/// Client configuration - connects to remote servers
#[derive(Debug, Deserialize, Clone)]
pub struct ClientConfig {
    /// Remote address to connect to
    pub addr: SocketAddr,

    /// Target clients/servers to forward data to
    pub target: Vec<String>,
}

impl Config {
    /// Load configuration from a TOML file
    ///
    /// # Arguments
    /// * `path` - Path to the configuration file
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(&path)
            .wrap_err_with(|| format!("Failed to read config file: {}", path.as_ref().display()))?;

        contents.parse()
    }

    /// Validate the configuration
    fn validate(&self) -> Result<()> {
        // Check that all target references exist
        let mut all_names = std::collections::HashSet::new();

        for name in self.server.keys() {
            all_names.insert(name.as_str());
        }

        for name in self.client.keys() {
            all_names.insert(name.as_str());
        }

        // Validate server targets
        for (name, server) in &self.server {
            for target in &server.target {
                if !all_names.contains(target.as_str()) {
                    return Err(color_eyre::eyre::eyre!(
                        "Server '{}' references unknown target '{}'",
                        name,
                        target
                    ));
                }
            }
        }

        // Validate client targets
        for (name, client) in &self.client {
            for target in &client.target {
                if !all_names.contains(target.as_str()) {
                    return Err(color_eyre::eyre::eyre!(
                        "Client '{}' references unknown target '{}'",
                        name,
                        target
                    ));
                }
            }
        }

        Ok(())
    }
}

impl FromStr for Config {
    type Err = color_eyre::Report;

    fn from_str(contents: &str) -> Result<Self, Self::Err> {
        let config: Config =
            toml::from_str(contents).wrap_err("Failed to parse TOML configuration")?;

        config.validate()?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_config() {
        let config_str = r#"
[server.ced]
port = 8000
target = ["masterA", "masterB"]

[client.masterA]
addr = "10.0.0.1:8000"
target = ["ced"]

[client.masterB]
addr = "10.0.0.5:8000"
target = ["ced"]
        "#;

        let config = Config::from_str(config_str).unwrap();

        assert_eq!(config.server.len(), 1);
        assert_eq!(config.client.len(), 2);

        let ced = config.server.get("ced").unwrap();
        assert_eq!(ced.port, 8000);
        assert_eq!(ced.target, vec!["masterA", "masterB"]);

        let master_a = config.client.get("masterA").unwrap();
        assert_eq!(master_a.addr.to_string(), "10.0.0.1:8000");
        assert_eq!(master_a.target, vec!["ced"]);
    }

    #[test]
    fn test_invalid_target_reference() {
        let config_str = r#"
[server.ced]
port = 8000
target = ["nonexistent"]
        "#;

        let result = Config::from_str(config_str);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown target"));
    }
}
