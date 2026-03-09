//! In-memory transaction store with bounded capacity.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use crate::types::{InstructionCount, ProgramCount, TransactionInfo, TransactionQuery, TransactionStats};

const MAX_TRANSACTIONS: usize = 5000;

/// In-memory transaction store
#[derive(Clone)]
pub struct TransactionStore {
    inner: Arc<RwLock<StoreInner>>,
}

struct StoreInner {
    /// Recent transactions, newest first
    transactions: VecDeque<TransactionInfo>,
    /// Track seen signatures to avoid duplicates
    seen: HashMap<String, ()>,
}

impl TransactionStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StoreInner {
                transactions: VecDeque::with_capacity(MAX_TRANSACTIONS),
                seen: HashMap::with_capacity(MAX_TRANSACTIONS),
            })),
        }
    }

    /// Add a transaction to the store. Returns true if it was new.
    pub fn add(&self, tx: TransactionInfo) -> bool {
        let mut inner = self.inner.write().unwrap();

        // Skip if already seen
        if inner.seen.contains_key(&tx.signature) {
            return false;
        }

        // Add to seen set
        inner.seen.insert(tx.signature.clone(), ());

        // Add to front (newest first)
        inner.transactions.push_front(tx);

        // Evict oldest if at capacity
        if inner.transactions.len() > MAX_TRANSACTIONS {
            if let Some(old) = inner.transactions.pop_back() {
                inner.seen.remove(&old.signature);
            }
        }

        true
    }

    /// Check if a transaction exists
    pub fn exists(&self, signature: &str) -> bool {
        let inner = self.inner.read().unwrap();
        inner.seen.contains_key(signature)
    }

    /// Update the run_id for an existing transaction. Returns true if updated.
    pub fn update_run_id(&self, signature: &str, run_id: &str) -> bool {
        let mut inner = self.inner.write().unwrap();
        for tx in inner.transactions.iter_mut() {
            if tx.signature == signature {
                tx.run_id = Some(run_id.to_string());
                return true;
            }
        }
        false
    }

    /// Query transactions with filters
    pub fn query(&self, query: &TransactionQuery) -> Vec<TransactionInfo> {
        let inner = self.inner.read().unwrap();

        let limit = query.limit.unwrap_or(100) as usize;
        let offset = query.offset.unwrap_or(0) as usize;

        let mut results: Vec<TransactionInfo> = inner
            .transactions
            .iter()
            .filter(|tx| {
                if let Some(ref run_id) = query.run_id {
                    if tx.run_id.as_ref() != Some(run_id) {
                        return false;
                    }
                }
                if let Some(ref signer) = query.signer {
                    if &tx.signer != signer {
                        return false;
                    }
                }
                if let Some(ref instruction_type) = query.instruction_type {
                    if &tx.instruction_type != instruction_type {
                        return false;
                    }
                }
                if let Some(ref program_name) = query.program_name {
                    if &tx.program_name != program_name {
                        return false;
                    }
                }
                if let Some(min_time) = query.min_time {
                    if tx.block_time.map(|t| t < min_time).unwrap_or(true) {
                        return false;
                    }
                }
                if let Some(max_time) = query.max_time {
                    if tx.block_time.map(|t| t > max_time).unwrap_or(true) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort by block_time descending (newest first)
        results.sort_by(|a, b| b.block_time.cmp(&a.block_time));

        results.into_iter().skip(offset).take(limit).collect()
    }

    /// Get statistics
    pub fn stats(&self, run_id_filter: Option<&str>) -> TransactionStats {
        let inner = self.inner.read().unwrap();

        let filtered: Vec<_> = inner
            .transactions
            .iter()
            .filter(|tx| {
                if let Some(rid) = run_id_filter {
                    tx.run_id.as_ref().map(|r| r == rid).unwrap_or(false)
                } else {
                    true
                }
            })
            .collect();

        // Count by instruction type
        let mut type_counts: HashMap<String, i64> = HashMap::new();
        for tx in &filtered {
            *type_counts.entry(tx.instruction_type.clone()).or_insert(0) += 1;
        }
        let mut by_instruction_type: Vec<InstructionCount> = type_counts
            .into_iter()
            .map(|(instruction_type, count)| InstructionCount { instruction_type, count })
            .collect();
        by_instruction_type.sort_by(|a, b| b.count.cmp(&a.count));

        // Count by program
        let mut program_counts: HashMap<String, i64> = HashMap::new();
        for tx in &filtered {
            *program_counts.entry(tx.program_name.clone()).or_insert(0) += 1;
        }
        let mut by_program: Vec<ProgramCount> = program_counts
            .into_iter()
            .map(|(program_name, count)| ProgramCount { program_name, count })
            .collect();
        by_program.sort_by(|a, b| b.count.cmp(&a.count));

        // Unique signers
        let unique_signers: i64 = filtered
            .iter()
            .map(|tx| &tx.signer)
            .collect::<std::collections::HashSet<_>>()
            .len() as i64;

        // Run IDs
        let mut run_ids: Vec<String> = inner
            .transactions
            .iter()
            .filter_map(|tx| tx.run_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        run_ids.sort();

        // Time range
        let times: Vec<i64> = filtered.iter().filter_map(|tx| tx.block_time).collect();
        let earliest_time = times.iter().min().copied();
        let latest_time = times.iter().max().copied();

        TransactionStats {
            total_count: filtered.len() as i64,
            by_instruction_type,
            by_program,
            unique_signers,
            run_ids,
            earliest_time,
            latest_time,
        }
    }

    /// Get transaction count
    pub fn len(&self) -> usize {
        self.inner.read().unwrap().transactions.len()
    }
}

impl Default for TransactionStore {
    fn default() -> Self {
        Self::new()
    }
}
