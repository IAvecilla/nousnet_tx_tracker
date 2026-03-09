//! Shared types for the transaction tracker.

use serde::{Deserialize, Serialize};

/// Decoded transaction information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    /// Transaction signature (base58)
    pub signature: String,
    /// Slot number
    pub slot: u64,
    /// Block time (Unix timestamp), if available
    pub block_time: Option<i64>,
    /// Transaction signer (first signer)
    pub signer: String,
    /// Program ID that was invoked
    pub program_id: String,
    /// Program name (coordinator, authorizer, etc.)
    pub program_name: String,
    /// Decoded instruction type (e.g., "join_run", "witness")
    pub instruction_type: String,
    /// Decoded instruction data as JSON (if available)
    pub instruction_data: Option<serde_json::Value>,
    /// Run ID extracted from coordinator transactions
    pub run_id: Option<String>,
    /// Client pubkey (for JoinRun, Witness, etc.)
    pub client_pubkey: Option<String>,
    /// Whether the transaction succeeded
    pub success: bool,
    /// Raw program logs
    pub logs: Vec<String>,
}

/// Query parameters for fetching transactions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionQuery {
    /// Filter by run ID
    pub run_id: Option<String>,
    /// Filter by signer
    pub signer: Option<String>,
    /// Filter by instruction type
    pub instruction_type: Option<String>,
    /// Filter by program name
    pub program_name: Option<String>,
    /// Minimum block time (Unix timestamp)
    pub min_time: Option<i64>,
    /// Maximum block time (Unix timestamp)
    pub max_time: Option<i64>,
    /// Limit number of results
    pub limit: Option<i64>,
    /// Offset for pagination
    pub offset: Option<i64>,
}

/// Statistics about tracked transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionStats {
    /// Total transaction count
    pub total_count: i64,
    /// Count by instruction type
    pub by_instruction_type: Vec<InstructionCount>,
    /// Count by program
    pub by_program: Vec<ProgramCount>,
    /// Unique signers count
    pub unique_signers: i64,
    /// Unique run IDs
    pub run_ids: Vec<String>,
    /// Earliest tracked transaction time
    pub earliest_time: Option<i64>,
    /// Latest tracked transaction time
    pub latest_time: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionCount {
    pub instruction_type: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramCount {
    pub program_name: String,
    pub count: i64,
}

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WsMessage {
    /// New transaction received
    NewTransaction(TransactionInfo),
    /// Stats update
    StatsUpdate(TransactionStats),
    /// Connection established
    Connected { message: String },
    /// Error message
    Error { message: String },
}

/// Query parameters for fetching historical transactions
#[derive(Debug, Clone, Deserialize)]
pub struct FetchHistoryQuery {
    /// Run ID to filter transactions for (required)
    pub run_id: Option<String>,
    /// Relative time like "1h", "1d", "1w" (default: "1d")
    pub since: Option<String>,
}

/// Response for historical transaction fetch
#[derive(Debug, Clone, Serialize)]
pub struct FetchHistoryResponse {
    /// Number of transactions fetched from RPC
    pub fetched_count: usize,
    /// Number of transactions matching the run_id
    pub matched_count: usize,
    /// Total transactions in store after fetch
    pub total_in_store: usize,
    /// Whether the fetch completed successfully
    pub complete: bool,
    /// Error message if fetch failed
    pub error: Option<String>,
    /// Matched transactions
    pub transactions: Vec<TransactionInfo>,
}

/// Parse a relative time string like "1h", "1d", "1w" into seconds
pub fn parse_relative_time(s: &str) -> Option<i64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }

    let (num_str, unit) = s.split_at(s.len().saturating_sub(1));
    let num: i64 = num_str.parse().ok()?;

    let multiplier = match unit {
        "h" => 3600,      // hours
        "d" => 86400,     // days
        "w" => 604800,    // weeks
        "m" => 60,        // minutes
        "s" => 1,         // seconds
        _ => return None,
    };

    Some(num * multiplier)
}
