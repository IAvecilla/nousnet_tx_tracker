//! Psyche Transaction Tracker - Real-time monitoring service for Psyche training runs.

mod config;
mod decoder;
mod fetcher;
mod server;
mod store;
mod types;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use crate::store::TransactionStore;
use crate::types::TransactionInfo;

#[derive(Parser)]
#[command(name = "psyche-tx-tracker")]
#[command(about = "Real-time transaction monitoring for Psyche training runs")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the transaction tracker server
    Serve {
        /// Solana RPC URL
        #[arg(long, default_value = "https://api.devnet.solana.com")]
        rpc: String,

        /// Solana WebSocket URL (derived from --rpc if not specified)
        #[arg(long)]
        ws_rpc: Option<String>,

        /// HTTP/WebSocket server port
        #[arg(long, default_value = "8765")]
        port: u16,

        /// Skip fetching recent historical transactions on start
        #[arg(long)]
        skip_recent: bool,

        /// Number of recent transactions to fetch per program (default: 50)
        #[arg(long, default_value = "50")]
        recent_limit: usize,

    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            rpc,
            ws_rpc,
            port,
            skip_recent,
            recent_limit,
        } => {
            // Derive WebSocket URL from RPC URL if not specified
            let ws_url = ws_rpc.unwrap_or_else(|| {
                rpc.replace("https://", "wss://")
                    .replace("http://", "ws://")
            });

            info!("Psyche Transaction Tracker starting...");
            info!("RPC URL: {}", rpc);
            info!("WebSocket URL: {}", ws_url);
            info!("Server port: {}", port);

            // Create in-memory store
            let store = Arc::new(TransactionStore::new());
            info!("In-memory transaction store initialized");

            // Create broadcast channel for real-time updates
            let (tx_sender, _) = broadcast::channel::<TransactionInfo>(1000);

            // Optionally fetch recent transactions
            if !skip_recent {
                info!("Fetching recent transactions...");
                if let Err(e) = fetcher::fetch_recent_transactions(
                    &rpc,
                    store.clone(),
                    &tx_sender,
                    recent_limit,
                ).await {
                    tracing::error!("Failed to fetch recent transactions: {}", e);
                }
            }

            // Start subscription in background
            let fetcher_store = store.clone();
            let fetcher_tx = tx_sender.clone();
            let rpc_clone = rpc.clone();
            let ws_clone = ws_url.clone();

            tokio::spawn(async move {
                if let Err(e) = fetcher::subscribe_to_programs(
                    &ws_clone,
                    &rpc_clone,
                    fetcher_store,
                    fetcher_tx,
                ).await {
                    tracing::error!("Fetcher error: {}", e);
                }
            });

            // Start HTTP/WebSocket server (blocks)
            server::start_server(
                port,
                store,
                tx_sender,
                rpc.clone(),
            ).await?;
        }
    }

    Ok(())
}
