#![warn(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations
)]
#![warn(clippy::pedantic)]

//! tcplex - Bidirectional TCP multiplexer with config-based routing

mod args;

use std::io::Write;

use clap::Parser;
use color_eyre::eyre::Result;
use log::{info, trace};
use owo_colors::OwoColorize;
use tcplex::{Config, Router};

use args::Args;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    // Parse CLI arguments
    let args = Args::parse();

    // Initialize logging based on verbosity flag
    env_logger::Builder::new()
        .filter_level(args.verbose.log_level_filter())
        .format(move |buf, record| log_format(buf, record, args.verbose.log_level_filter()))
        .init();

    trace!("Parsed Args:\n{args:#?}");

    info!("Starting tcplex");
    info!("Loading configuration from: {}", args.config.display());

    // Load configuration
    let config = Config::from_file(&args.config)?;
    info!("Configuration loaded successfully");
    info!("Servers: {:?}", config.server.keys().collect::<Vec<_>>());
    info!("Clients: {:?}", config.client.keys().collect::<Vec<_>>());

    // Create and run the router
    let router = Router::new(config);
    router.run().await
}

/// Formats the log messages in a minimalistic way, since we don't have a lot of output.
fn log_format(
    buf: &mut env_logger::fmt::Formatter,
    record: &log::Record,
    filter: log::LevelFilter,
) -> std::io::Result<()> {
    let level = record.level();
    let level_char = match level {
        log::Level::Trace => 'T',
        log::Level::Debug => 'D',
        log::Level::Info => 'I',
        log::Level::Warn => 'W',
        log::Level::Error => 'E',
    };

    let colored_level = match level {
        log::Level::Trace => level_char.white().to_string(),
        log::Level::Debug => level_char.cyan().to_string(),
        log::Level::Info => level_char.green().to_string(),
        log::Level::Warn => level_char.yellow().to_string(),
        log::Level::Error => level_char.red().to_string(),
    };

    // When running at default verbosity (Info), omit the level prefix for Info messages
    // to keep output clean. Show prefixes for all other levels or when in verbose mode.
    if level == log::Level::Info && filter == log::LevelFilter::Info {
        writeln!(buf, "{}", record.args())
    } else {
        writeln!(buf, "{}: {}", colored_level, record.args())
    }
}
