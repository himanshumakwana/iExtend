//! fake-ipad — a CLI tool that impersonates an iPad during the SPAKE2
//! pairing handshake for testing and development.
//!
//! Subcommands:
//!   pair --host <ip:port> --pin <pin>   TCP-connect and run SPAKE2 client.
//!   show                                Print the last pair record.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fake_ipad::pairing_client::run_pairing_client;
use fake_ipad::{last_pair_path, PairRecord};

/// Fake-ipad CLI — SPAKE2 pairing test client for iExtend.
#[derive(Parser, Debug)]
#[command(name = "fake-ipad", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the SPAKE2 pairing handshake against an iextendd instance.
    Pair {
        /// Daemon's pairing TCP address (e.g. 127.0.0.1:53421).
        #[arg(long)]
        host: String,

        /// 4-digit numeric PIN shown by the daemon.
        #[arg(long)]
        pin: String,
    },
    /// Print the last saved pair record from disk.
    Show,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Pair { host, pin } => {
            run_pairing_client(&host, &pin).await?;
        }
        Commands::Show => {
            cmd_show()?;
        }
    }

    Ok(())
}

fn cmd_show() -> Result<()> {
    let path = last_pair_path();
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    let record: PairRecord =
        serde_json::from_slice(&bytes).with_context(|| "invalid JSON in last-pair.json")?;
    println!("{}", serde_json::to_string_pretty(&record)?);
    Ok(())
}
