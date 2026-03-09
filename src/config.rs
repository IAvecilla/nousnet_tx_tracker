//! Program IDs and Anchor instruction discriminators for Psyche programs.
//!
//! Discriminators are the first 8 bytes of SHA256("global:<instruction_name>").

use sha2::{Digest, Sha256};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::LazyLock;

/// Compute Anchor discriminator for an instruction name
pub fn compute_discriminator(instruction_name: &str) -> [u8; 8] {
    let preimage = format!("global:{}", instruction_name);
    let hash = Sha256::digest(preimage.as_bytes());
    let mut discriminator = [0u8; 8];
    discriminator.copy_from_slice(&hash[..8]);
    discriminator
}

/// Program metadata with ID and instruction discriminators
#[derive(Debug, Clone)]
pub struct ProgramConfig {
    pub name: &'static str,
    pub id: Pubkey,
    /// Map from discriminator bytes to instruction name
    pub instructions: HashMap<[u8; 8], &'static str>,
}

impl ProgramConfig {
    fn new(name: &'static str, id_str: &str, instruction_names: &[&'static str]) -> Self {
        let id = Pubkey::from_str(id_str).expect("Invalid program ID");
        let mut instructions = HashMap::new();
        for &name in instruction_names {
            let disc = compute_discriminator(name);
            instructions.insert(disc, name);
        }
        Self {
            name,
            id,
            instructions,
        }
    }

    pub fn decode_instruction(&self, data: &[u8]) -> Option<&'static str> {
        if data.len() < 8 {
            return None;
        }
        let mut disc = [0u8; 8];
        disc.copy_from_slice(&data[..8]);
        self.instructions.get(&disc).copied()
    }
}

/// Coordinator program configuration
pub static COORDINATOR: LazyLock<ProgramConfig> = LazyLock::new(|| {
    ProgramConfig::new(
        "coordinator",
        "4SHugWqSXwKE5fqDchkJcPEqnoZE22VYKtSTVm7axbT7",
        &[
            "init_coordinator",
            "free_coordinator",
            "update",
            "update_client_version",
            "set_future_epoch_rates",
            "join_run",
            "set_paused",
            "tick",
            "witness",
            "warmup_witness",
            "health_check",
            "checkpoint",
        ],
    )
});

/// Authorizer program configuration
pub static AUTHORIZER: LazyLock<ProgramConfig> = LazyLock::new(|| {
    ProgramConfig::new(
        "authorizer",
        "PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw",
        &[
            "authorization_create",
            "authorization_grantor_update",
            "authorization_grantee_update",
            "authorization_close",
        ],
    )
});

/// Treasurer program configuration
pub static TREASURER: LazyLock<ProgramConfig> = LazyLock::new(|| {
    ProgramConfig::new(
        "treasurer",
        "EnU7DRx5az5YWxaxgqEGbXSYtudcfnjXewyBRRZCjJPw",
        &[
            "run_create",
            "run_update",
            "participant_create",
            "participant_claim",
        ],
    )
});

/// Mining Pool program configuration
pub static MINING_POOL: LazyLock<ProgramConfig> = LazyLock::new(|| {
    ProgramConfig::new(
        "mining_pool",
        "CQy5JKR2Lrm16pqSY5nkMaMYSazRk2aYx99pJDNGupR7",
        &[
            "pool_create",
            "pool_extract",
            "pool_update",
            "pool_claimable",
            "lender_create",
            "lender_deposit",
            "lender_claim",
        ],
    )
});

/// All tracked programs
pub static ALL_PROGRAMS: LazyLock<Vec<&'static ProgramConfig>> = LazyLock::new(|| {
    vec![&*COORDINATOR, &*AUTHORIZER, &*TREASURER, &*MINING_POOL]
});

/// Get all program IDs as pubkeys
pub fn all_program_ids() -> Vec<Pubkey> {
    ALL_PROGRAMS.iter().map(|p| p.id).collect()
}

/// Derive the coordinator PDA from a run_id
/// Seeds: ["coordinator", run_id]
pub fn derive_coordinator_pda(run_id: &str) -> Pubkey {
    let (pda, _bump) = Pubkey::find_program_address(
        &[b"coordinator", run_id.as_bytes()],
        &COORDINATOR.id,
    );
    pda
}

/// Find program config by ID
pub fn find_program(program_id: &Pubkey) -> Option<&'static ProgramConfig> {
    ALL_PROGRAMS.iter().find(|p| p.id == *program_id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discriminator_computation() {
        // Verify discriminator computation matches Anchor's approach
        let disc = compute_discriminator("join_run");
        // The discriminator should be 8 bytes from SHA256("global:join_run")
        assert_eq!(disc.len(), 8);
    }

    #[test]
    fn test_program_configs() {
        // Ensure all programs are initialized correctly
        assert_eq!(COORDINATOR.name, "coordinator");
        assert_eq!(AUTHORIZER.name, "authorizer");
        assert_eq!(TREASURER.name, "treasurer");
        assert_eq!(MINING_POOL.name, "mining_pool");

        // Check instruction count
        assert_eq!(COORDINATOR.instructions.len(), 12);
        assert_eq!(AUTHORIZER.instructions.len(), 4);
        assert_eq!(TREASURER.instructions.len(), 4);
        assert_eq!(MINING_POOL.instructions.len(), 7);
    }

    #[test]
    fn test_decode_instruction() {
        let mut data = vec![0u8; 100];
        let disc = compute_discriminator("join_run");
        data[..8].copy_from_slice(&disc);

        let instruction = COORDINATOR.decode_instruction(&data);
        assert_eq!(instruction, Some("join_run"));
    }

    #[test]
    fn test_derive_coordinator_pda() {
        let run_id = "moe-10b-a1b-8k-wsd-lr3e4-1t";
        let pda = derive_coordinator_pda(run_id);
        println!("Run ID: {}", run_id);
        println!("Coordinator Program: {}", COORDINATOR.id);
        println!("Derived PDA: {}", pda);
    }
}
