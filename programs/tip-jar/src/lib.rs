//! Tip jar for any SPL token (classic Token program).
//!
//! * `initialize(owner)` creates a jar PDA (`["jar", owner, mint]`) plus a vault token
//!   account PDA (`["vault", jar]`) whose authority is the jar.
//! * `tip(amount)` moves `amount > 0` tokens from the sender into the vault.
//! * `withdraw(amount)` lets only the jar owner move tokens out, up to the vault balance.

pub mod error;
pub mod processor;
pub mod state;

use pinocchio::{entrypoint, AccountView, Address, ProgramResult};

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    processor::process(program_id, accounts, instruction_data)
}
