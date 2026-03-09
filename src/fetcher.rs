//! Transaction fetching from Solana RPC.

use anyhow::{Context, Result};
use futures::StreamExt;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcTransactionConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::signature::Signature;
use solana_transaction_status::UiTransactionEncoding;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::config::all_program_ids;
use crate::decoder::decode_transaction;
use crate::store::TransactionStore;
use crate::types::TransactionInfo;

/// Subscribe to real-time transaction logs for all tracked programs
pub async fn subscribe_to_programs(
    ws_url: &str,
    rpc_url: &str,
    store: Arc<TransactionStore>,
    tx_sender: broadcast::Sender<TransactionInfo>,
) -> Result<()> {
    let program_ids = all_program_ids();

    info!("Subscribing to {} programs via WebSocket", program_ids.len());

    // Create a channel for log notifications
    let (log_tx, mut log_rx) = mpsc::channel::<(String, Vec<String>)>(1000);

    // Spawn subscription tasks for each program
    for program_id in program_ids {
        let ws_url = ws_url.to_string();
        let log_tx = log_tx.clone();
        let program_id_str = program_id.to_string();

        tokio::spawn(async move {
            loop {
                match subscribe_single_program(&ws_url, &program_id_str, log_tx.clone()).await {
                    Ok(()) => {
                        info!("Subscription ended for {}, reconnecting...", program_id_str);
                    }
                    Err(e) => {
                        error!("Subscription error for {}: {}", program_id_str, e);
                    }
                }

                // Reconnect with backoff
                sleep(Duration::from_secs(5)).await;
            }
        });
    }

    // Process incoming log notifications
    let rpc_client = RpcClient::new(rpc_url.to_string());

    while let Some((signature, _logs)) = log_rx.recv().await {
        // Check if we already have this transaction
        if store.exists(&signature) {
            continue;
        }

        // Fetch full transaction and decode
        match fetch_and_decode_transaction(&rpc_client, &signature).await {
            Ok(decoded) => {
                for tx_info in decoded {
                    debug!("New transaction: {} - {}", tx_info.instruction_type, tx_info.signature);
                    if store.add(tx_info.clone()) {
                        let _ = tx_sender.send(tx_info);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to process transaction {}: {}", signature, e);
            }
        }
    }

    Ok(())
}

/// Fetch and decode a single transaction
async fn fetch_and_decode_transaction(
    client: &RpcClient,
    signature: &str,
) -> Result<Vec<TransactionInfo>> {
    let sig = Signature::from_str(signature)?;

    let config = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::JsonParsed),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };

    let tx = client
        .get_transaction_with_config(&sig, config)
        .await
        .context("Failed to fetch transaction")?;

    Ok(decode_transaction(signature, &tx))
}

/// Subscribe to a single program's logs
async fn subscribe_single_program(
    ws_url: &str,
    program_id: &str,
    log_tx: mpsc::Sender<(String, Vec<String>)>,
) -> Result<()> {
    info!("Connecting to WebSocket for program {}", program_id);

    let client = PubsubClient::new(ws_url)
        .await
        .context("Failed to connect to WebSocket")?;

    let config = RpcTransactionLogsConfig {
        commitment: Some(CommitmentConfig::confirmed()),
    };

    let filter = RpcTransactionLogsFilter::Mentions(vec![program_id.to_string()]);

    let (mut stream, _unsub) = client
        .logs_subscribe(filter, config)
        .await
        .context("Failed to subscribe to logs")?;

    info!("Subscribed to logs for program {}", program_id);

    while let Some(log_result) = stream.next().await {
        let signature = log_result.value.signature;
        let logs = log_result.value.logs;

        if let Err(e) = log_tx.send((signature.clone(), logs)).await {
            error!("Failed to send log notification: {}", e);
            break;
        }
    }

    Ok(())
}

/// Fetch recent historical transactions (optional, for initial load)
pub async fn fetch_recent_transactions(
    rpc_url: &str,
    store: Arc<TransactionStore>,
    tx_sender: &broadcast::Sender<TransactionInfo>,
    limit: usize,
) -> Result<()> {
    let client = RpcClient::new(rpc_url.to_string());
    let program_ids = all_program_ids();

    info!("Fetching recent transactions for {} programs", program_ids.len());

    for program_id in program_ids {
        let config = solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
            before: None,
            until: None,
            limit: Some(limit),
            commitment: Some(CommitmentConfig::confirmed()),
        };

        let signatures = match client.get_signatures_for_address_with_config(&program_id, config).await {
            Ok(sigs) => sigs,
            Err(e) => {
                warn!("Failed to fetch signatures for {}: {}", program_id, e);
                continue;
            }
        };

        info!("Fetched {} signatures for {}", signatures.len(), program_id);

        for sig_info in signatures {
            if store.exists(&sig_info.signature) {
                continue;
            }

            match fetch_and_decode_transaction(&client, &sig_info.signature).await {
                Ok(decoded) => {
                    for tx_info in decoded {
                        if store.add(tx_info.clone()) {
                            let _ = tx_sender.send(tx_info);
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to fetch transaction {}: {}", sig_info.signature, e);
                }
            }

            // Small delay to avoid rate limiting
            sleep(Duration::from_millis(50)).await;
        }
    }

    info!("Initial fetch complete, {} transactions in store", store.len());
    Ok(())
}

/// Configuration for historical transaction fetching
pub struct HistoricalFetchConfig {
    /// Run ID to filter for
    pub run_id: String,
    /// Coordinator PDA derived from run_id
    pub coordinator_pda: String,
    /// Unix timestamp to fetch transactions since
    pub since_timestamp: i64,
    /// Number of signatures to fetch per request
    pub batch_size: usize,
    /// Delay between RPC calls in milliseconds
    pub rate_limit_ms: u64,
}

/// Result of historical transaction fetch
pub struct HistoricalFetchResult {
    /// Number of transactions fetched from RPC
    pub fetched_count: usize,
    /// Number of transactions matching the run_id
    pub matched_count: usize,
    /// Total transactions in store
    pub total_in_store: usize,
    /// Whether the fetch completed without errors
    pub complete: bool,
    /// Error message if any
    pub error: Option<String>,
    /// Matched transactions
    pub transactions: Vec<TransactionInfo>,
}

/// Fetch historical transactions for a specific run_id
pub async fn fetch_historical_transactions(
    rpc_url: &str,
    store: Arc<TransactionStore>,
    tx_sender: &broadcast::Sender<TransactionInfo>,
    config: HistoricalFetchConfig,
) -> HistoricalFetchResult {
    let client = RpcClient::new(rpc_url.to_string());

    info!(
        "Fetching historical transactions for run_id={} (coordinator_pda={}) since={}",
        config.run_id, config.coordinator_pda, config.since_timestamp
    );

    // Fetch transactions directly for the coordinator PDA
    // This is more efficient than fetching all program transactions
    let coordinator_pda = match solana_sdk::pubkey::Pubkey::from_str(&config.coordinator_pda) {
        Ok(pda) => pda,
        Err(e) => {
            return HistoricalFetchResult {
                fetched_count: 0,
                matched_count: 0,
                total_in_store: store.len(),
                complete: false,
                error: Some(format!("Invalid coordinator PDA: {}", e)),
                transactions: vec![],
            };
        }
    };

    let mut total_fetched = 0;
    let mut matched_transactions = Vec::new();
    let mut had_error = false;
    let mut error_message = None;

    match fetch_pda_history(
        &client,
        &coordinator_pda,
        store.clone(),
        tx_sender,
        &config,
    )
    .await
    {
        Ok((fetched, matched)) => {
            total_fetched = fetched;
            matched_transactions = matched;
        }
        Err(e) => {
            warn!("Error fetching history for coordinator PDA: {}", e);
            had_error = true;
            error_message = Some(e.to_string());
        }
    }

    // Sort matched transactions by block_time descending
    matched_transactions.sort_by(|a, b| b.block_time.cmp(&a.block_time));

    info!(
        "Historical fetch complete: fetched={}, matched={}, store_size={}",
        total_fetched,
        matched_transactions.len(),
        store.len()
    );

    HistoricalFetchResult {
        fetched_count: total_fetched,
        matched_count: matched_transactions.len(),
        total_in_store: store.len(),
        complete: !had_error,
        error: error_message,
        transactions: matched_transactions,
    }
}

/// Fetch historical transactions for a coordinator PDA with pagination
async fn fetch_pda_history(
    client: &RpcClient,
    pda: &solana_sdk::pubkey::Pubkey,
    store: Arc<TransactionStore>,
    tx_sender: &broadcast::Sender<TransactionInfo>,
    config: &HistoricalFetchConfig,
) -> Result<(usize, Vec<TransactionInfo>)> {
    let mut fetched_count = 0;
    let mut matched_transactions = Vec::new();
    let mut before: Option<Signature> = None;

    info!("Fetching transactions for PDA: {}", pda);

    loop {
        let sig_config = solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
            before,
            until: None,
            limit: Some(config.batch_size),
            commitment: Some(CommitmentConfig::confirmed()),
        };

        let signatures = client
            .get_signatures_for_address_with_config(pda, sig_config)
            .await
            .context("Failed to fetch signatures for PDA")?;

        if signatures.is_empty() {
            break;
        }

        info!("Fetched {} signatures for PDA", signatures.len());

        let mut reached_time_limit = false;

        for sig_info in &signatures {
            // Check block_time before fetching full transaction
            if let Some(block_time) = sig_info.block_time {
                if block_time < config.since_timestamp {
                    reached_time_limit = true;
                    break;
                }
            }

            // Check if already in store - if so, just update the run_id
            if store.exists(&sig_info.signature) {
                // Update the run_id for existing transaction
                store.update_run_id(&sig_info.signature, &config.run_id);
                continue;
            }

            // Fetch and decode the transaction
            match fetch_and_decode_transaction_public(client, &sig_info.signature).await {
                Ok(decoded) => {
                    fetched_count += decoded.len();
                    for mut tx_info in decoded {
                        // Set the run_id since this transaction involves the coordinator PDA
                        tx_info.run_id = Some(config.run_id.clone());

                        if store.add(tx_info.clone()) {
                            let _ = tx_sender.send(tx_info.clone());
                        }

                        matched_transactions.push(tx_info);
                    }
                }
                Err(e) => {
                    debug!("Failed to fetch transaction {}: {}", sig_info.signature, e);
                }
            }

            // Rate limiting
            sleep(Duration::from_millis(config.rate_limit_ms)).await;
        }

        if reached_time_limit || signatures.len() < config.batch_size {
            break;
        }

        // Set cursor for next page
        if let Some(last) = signatures.last() {
            before = Some(Signature::from_str(&last.signature)?);
        } else {
            break;
        }
    }

    Ok((fetched_count, matched_transactions))
}

/// Fetch historical transactions for a single program with pagination (unused but kept for reference)
#[allow(dead_code)]
async fn fetch_program_history(
    client: &RpcClient,
    program_id: &solana_sdk::pubkey::Pubkey,
    store: Arc<TransactionStore>,
    tx_sender: &broadcast::Sender<TransactionInfo>,
    config: &HistoricalFetchConfig,
) -> Result<(usize, Vec<TransactionInfo>)> {
    let mut fetched_count = 0;
    let mut matched_transactions = Vec::new();
    let mut before: Option<Signature> = None;

    loop {
        let sig_config = solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
            before,
            until: None,
            limit: Some(config.batch_size),
            commitment: Some(CommitmentConfig::confirmed()),
        };

        let signatures = client
            .get_signatures_for_address_with_config(program_id, sig_config)
            .await
            .context("Failed to fetch signatures")?;

        if signatures.is_empty() {
            break;
        }

        let mut reached_time_limit = false;

        for sig_info in &signatures {
            // Check block_time before fetching full transaction
            if let Some(block_time) = sig_info.block_time {
                if block_time < config.since_timestamp {
                    reached_time_limit = true;
                    break;
                }
            }

            // Skip if already in store
            if store.exists(&sig_info.signature) {
                continue;
            }

            // Fetch and decode the transaction
            match fetch_and_decode_transaction_public(client, &sig_info.signature).await {
                Ok(decoded) => {
                    fetched_count += decoded.len();
                    for mut tx_info in decoded {
                        // Check if the coordinator PDA is in the transaction's logs
                        // The coordinator PDA appears in the invoke log line
                        let matches_run_id = tx_info.logs.iter().any(|log| {
                            log.contains(&config.coordinator_pda)
                        }) || tx_info.run_id.as_ref().map(|r| r == &config.run_id).unwrap_or(false);

                        // If it matches, set the run_id on the transaction
                        if matches_run_id {
                            tx_info.run_id = Some(config.run_id.clone());
                        }

                        if store.add(tx_info.clone()) {
                            let _ = tx_sender.send(tx_info.clone());
                        }

                        if matches_run_id {
                            matched_transactions.push(tx_info);
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to fetch transaction {}: {}", sig_info.signature, e);
                }
            }

            // Rate limiting
            sleep(Duration::from_millis(config.rate_limit_ms)).await;
        }

        if reached_time_limit || signatures.len() < config.batch_size {
            break;
        }

        // Set cursor for next page
        if let Some(last) = signatures.last() {
            before = Some(Signature::from_str(&last.signature)?);
        } else {
            break;
        }
    }

    Ok((fetched_count, matched_transactions))
}

/// Public version of fetch_and_decode_transaction for use in historical fetching
async fn fetch_and_decode_transaction_public(
    client: &RpcClient,
    signature: &str,
) -> Result<Vec<TransactionInfo>> {
    let sig = Signature::from_str(signature)?;

    let config = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::JsonParsed),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };

    let tx = client
        .get_transaction_with_config(&sig, config)
        .await
        .context("Failed to fetch transaction")?;

    Ok(decode_transaction(signature, &tx))
}
