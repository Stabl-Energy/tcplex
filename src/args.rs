//! Command-line argument parsing for tcplex

use std::path::PathBuf;

use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};

/// tcplex - Bidirectional TCP multiplexer with config-based routing
#[derive(Parser, Debug)]
#[command(name = "tcplex")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Path to the configuration file
    #[arg(short, long, default_value = "tcplex.toml")]
    pub config: PathBuf,

    /// Verbosity level
    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,
}
