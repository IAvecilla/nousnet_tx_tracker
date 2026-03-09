//! Instruction decoding using Anchor discriminators.

use solana_sdk::pubkey::Pubkey;
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, UiInstruction, UiMessage,
    UiParsedInstruction,
};
use std::str::FromStr;

use crate::config::find_program;
use crate::types::TransactionInfo;

/// Decode a transaction into TransactionInfo
pub fn decode_transaction(
    signature: &str,
    tx: &EncodedConfirmedTransactionWithStatusMeta,
) -> Vec<TransactionInfo> {
    let mut results = Vec::new();

    let slot = tx.slot;
    let block_time = tx.block_time;

    // Get transaction meta and check success
    let meta = match &tx.transaction.meta {
        Some(m) => m,
        None => return results,
    };

    let success = meta.err.is_none();

    // Extract logs
    use solana_transaction_status::option_serializer::OptionSerializer;
    let logs: Vec<String> = match &meta.log_messages {
        OptionSerializer::Some(logs) => logs.clone(),
        OptionSerializer::Skip | OptionSerializer::None => Vec::new(),
    };

    // Get the transaction message
    let message = match &tx.transaction.transaction {
        solana_transaction_status::EncodedTransaction::Json(ui_tx) => &ui_tx.message,
        _ => return results,
    };

    // Extract account keys and signer
    let (account_keys, signer) = match message {
        UiMessage::Parsed(parsed) => {
            let keys: Vec<String> = parsed
                .account_keys
                .iter()
                .map(|k| k.pubkey.clone())
                .collect();
            let signer = keys.first().cloned().unwrap_or_default();
            (keys, signer)
        }
        UiMessage::Raw(raw) => {
            let signer = raw.account_keys.first().cloned().unwrap_or_default();
            (raw.account_keys.clone(), signer)
        }
    };

    // Process instructions
    let instructions = match message {
        UiMessage::Parsed(parsed) => &parsed.instructions,
        UiMessage::Raw(raw) => {
            // For raw messages, we need to decode differently
            for ix in &raw.instructions {
                if let Some(program_idx) = Some(ix.program_id_index as usize) {
                    if program_idx < account_keys.len() {
                        let program_id_str = &account_keys[program_idx];
                        if let Ok(program_id) = Pubkey::from_str(program_id_str) {
                            if let Some(program_config) = find_program(&program_id) {
                                let data = bs58::decode(&ix.data).into_vec().unwrap_or_default();
                                if let Some(instruction_type) =
                                    program_config.decode_instruction(&data)
                                {
                                    let (run_id, client_pubkey) = extract_context(
                                        instruction_type,
                                        &data,
                                        &ix.accounts,
                                        &account_keys,
                                        &logs,
                                    );

                                    results.push(TransactionInfo {
                                        signature: signature.to_string(),
                                        slot,
                                        block_time,
                                        signer: signer.clone(),
                                        program_id: program_id_str.clone(),
                                        program_name: program_config.name.to_string(),
                                        instruction_type: instruction_type.to_string(),
                                        instruction_data: None,
                                        run_id,
                                        client_pubkey,
                                        success,
                                        logs: logs.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            return results;
        }
    };

    for instruction in instructions {
        match instruction {
            UiInstruction::Compiled(compiled) => {
                let program_idx = compiled.program_id_index as usize;
                if program_idx < account_keys.len() {
                    let program_id_str = &account_keys[program_idx];
                    if let Ok(program_id) = Pubkey::from_str(program_id_str) {
                        if let Some(program_config) = find_program(&program_id) {
                            let data = bs58::decode(&compiled.data).into_vec().unwrap_or_default();
                            if let Some(instruction_type) =
                                program_config.decode_instruction(&data)
                            {
                                let (run_id, client_pubkey) = extract_context(
                                    instruction_type,
                                    &data,
                                    &compiled.accounts,
                                    &account_keys,
                                    &logs,
                                );

                                results.push(TransactionInfo {
                                    signature: signature.to_string(),
                                    slot,
                                    block_time,
                                    signer: signer.clone(),
                                    program_id: program_id_str.clone(),
                                    program_name: program_config.name.to_string(),
                                    instruction_type: instruction_type.to_string(),
                                    instruction_data: None,
                                    run_id,
                                    client_pubkey,
                                    success,
                                    logs: logs.clone(),
                                });
                            }
                        }
                    }
                }
            }
            UiInstruction::Parsed(parsed) => {
                match parsed {
                    UiParsedInstruction::Parsed(_) => {
                        // Standard Solana program, not our programs
                    }
                    UiParsedInstruction::PartiallyDecoded(partial) => {
                        if let Ok(program_id) = Pubkey::from_str(&partial.program_id) {
                            if let Some(program_config) = find_program(&program_id) {
                                let data =
                                    bs58::decode(&partial.data).into_vec().unwrap_or_default();
                                if let Some(instruction_type) =
                                    program_config.decode_instruction(&data)
                                {
                                    let (run_id, client_pubkey) = extract_context_from_pubkeys(
                                        instruction_type,
                                        &data,
                                        &partial.accounts,
                                        &logs,
                                    );

                                    results.push(TransactionInfo {
                                        signature: signature.to_string(),
                                        slot,
                                        block_time,
                                        signer: signer.clone(),
                                        program_id: partial.program_id.clone(),
                                        program_name: program_config.name.to_string(),
                                        instruction_type: instruction_type.to_string(),
                                        instruction_data: None,
                                        run_id,
                                        client_pubkey,
                                        success,
                                        logs: logs.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    results
}

/// Extract run_id and client_pubkey from instruction context
fn extract_context(
    instruction_type: &str,
    _data: &[u8],
    account_indices: &[u8],
    account_keys: &[String],
    logs: &[String],
) -> (Option<String>, Option<String>) {
    let accounts: Vec<&String> = account_indices
        .iter()
        .filter_map(|&idx| account_keys.get(idx as usize))
        .collect();

    extract_context_inner(instruction_type, &accounts, logs)
}

/// Extract context when we have pubkey strings directly
fn extract_context_from_pubkeys(
    instruction_type: &str,
    _data: &[u8],
    accounts: &[String],
    logs: &[String],
) -> (Option<String>, Option<String>) {
    let account_refs: Vec<&String> = accounts.iter().collect();
    extract_context_inner(instruction_type, &account_refs, logs)
}

/// Inner extraction logic
fn extract_context_inner(
    instruction_type: &str,
    accounts: &[&String],
    logs: &[String],
) -> (Option<String>, Option<String>) {
    let mut run_id = None;
    let mut client_pubkey = None;

    // Try to extract run_id from logs
    // Coordinator logs often include "run_id: <id>" or similar
    for log in logs {
        if log.contains("run_id:") || log.contains("run_id =") {
            if let Some(idx) = log.find("run_id") {
                let rest = &log[idx..];
                // Try to extract the value after "run_id:" or "run_id ="
                if let Some(colon_idx) = rest.find(':').or_else(|| rest.find('=')) {
                    let value_start = colon_idx + 1;
                    let value = rest[value_start..].trim();
                    // Take until whitespace or end
                    let value_end = value
                        .find(|c: char| c.is_whitespace() || c == ',')
                        .unwrap_or(value.len());
                    let extracted = value[..value_end].trim_matches('"').to_string();
                    if !extracted.is_empty() {
                        run_id = Some(extracted);
                        break;
                    }
                }
            }
        }
    }

    // Extract client pubkey based on instruction type
    // For coordinator instructions, the user/signer is typically the first account
    match instruction_type {
        "join_run" | "witness" | "warmup_witness" | "health_check" | "tick" | "checkpoint" => {
            // User is typically the first account (signer)
            if let Some(&first) = accounts.first() {
                client_pubkey = Some(first.clone());
            }
        }
        _ => {}
    }

    (run_id, client_pubkey)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_run_id_from_logs() {
        let logs = vec![
            "Program log: Instruction: JoinRun".to_string(),
            "Program log: run_id: test-run-123".to_string(),
        ];

        let (run_id, _) = extract_context_inner("join_run", &[], &logs);
        assert_eq!(run_id, Some("test-run-123".to_string()));
    }
}
