use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer as SystemTransfer};
use pinocchio_token::{
    instructions::{InitializeAccount3, Transfer},
    state::{Account as TokenAccount, Mint},
};

use crate::{
    error::TipJarError,
    state::{Jar, JAR_LEN, JAR_SEED, VAULT_SEED},
};

pub const IX_INITIALIZE: u8 = 0;
pub const IX_TIP: u8 = 1;
pub const IX_WITHDRAW: u8 = 2;

pub fn process(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let (discriminator, data) = instruction_data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;

    match *discriminator {
        IX_INITIALIZE => initialize(program_id, accounts, data),
        IX_TIP => tip(program_id, accounts, data),
        IX_WITHDRAW => withdraw(program_id, accounts, data),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

fn parse_amount(data: &[u8]) -> Result<u64, ProgramError> {
    let bytes: [u8; 8] = data
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    Ok(u64::from_le_bytes(bytes))
}

fn check_token_program(token_program: &AccountView) -> ProgramResult {
    if token_program.address() != &pinocchio_token::ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}

/// Creates a PDA-owned account. Works even if someone pre-funded the address with lamports
/// (a plain `CreateAccount` fails in that case, which would let anyone block initialization).
fn create_pda_account(
    payer: &AccountView,
    account: &AccountView,
    space: usize,
    owner: &Address,
    signers: &[Signer],
) -> ProgramResult {
    let required = Rent::get()?.try_minimum_balance(space)?;

    if account.lamports() == 0 {
        return CreateAccount {
            from: payer,
            to: account,
            lamports: required,
            space: space as u64,
            owner,
        }
        .invoke_signed(signers);
    }

    if account.data_len() != 0 || !account.owned_by(&pinocchio_system::ID) {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    let missing = required.saturating_sub(account.lamports());
    if missing > 0 {
        SystemTransfer {
            from: payer,
            to: account,
            lamports: missing,
        }
        .invoke()?;
    }
    Allocate {
        account,
        space: space as u64,
    }
    .invoke_signed(signers)?;
    Assign { account, owner }.invoke_signed(signers)
}

/// Accounts: payer (signer, writable), jar (writable), vault (writable), mint,
/// system program, token program. Data: owner address (32 bytes).
fn initialize(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [payer, jar, vault, mint, system_program, token_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let owner_bytes: [u8; 32] = data
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let owner = Address::from(owner_bytes);

    if !payer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if system_program.address() != &pinocchio_system::ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    check_token_program(token_program)?;
    if !mint.owned_by(&pinocchio_token::ID) || mint.data_len() != Mint::LEN {
        return Err(ProgramError::InvalidAccountData);
    }

    let (expected_jar, jar_bump) = Address::derive_program_address(
        &[JAR_SEED, owner.as_ref(), mint.address().as_ref()],
        program_id,
    )
    .ok_or(ProgramError::InvalidSeeds)?;
    if jar.address() != &expected_jar {
        return Err(ProgramError::InvalidSeeds);
    }

    let (expected_vault, vault_bump) =
        Address::derive_program_address(&[VAULT_SEED, jar.address().as_ref()], program_id)
            .ok_or(ProgramError::InvalidSeeds)?;
    if vault.address() != &expected_vault {
        return Err(ProgramError::InvalidSeeds);
    }

    let jar_bump_seed = [jar_bump];
    let jar_seeds = [
        Seed::from(JAR_SEED),
        Seed::from(owner.as_ref()),
        Seed::from(mint.address().as_ref()),
        Seed::from(&jar_bump_seed),
    ];
    create_pda_account(
        payer,
        jar,
        JAR_LEN,
        program_id,
        &[Signer::from(&jar_seeds)],
    )?;

    let vault_bump_seed = [vault_bump];
    let vault_seeds = [
        Seed::from(VAULT_SEED),
        Seed::from(jar.address().as_ref()),
        Seed::from(&vault_bump_seed),
    ];
    create_pda_account(
        payer,
        vault,
        TokenAccount::LEN,
        &pinocchio_token::ID,
        &[Signer::from(&vault_seeds)],
    )?;

    InitializeAccount3::new(vault, mint, jar.address()).invoke()?;

    Jar {
        bump: jar_bump,
        owner,
        mint: mint.address().clone(),
        vault: vault.address().clone(),
    }
    .store(jar)
}

/// Accounts: tipper (signer), tipper token account (writable), jar, vault (writable),
/// token program. Data: amount (u64 LE).
fn tip(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [tipper, source, jar, vault, token_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let amount = parse_amount(data)?;
    if amount == 0 {
        return Err(TipJarError::ZeroAmount.into());
    }
    if !tipper.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    check_token_program(token_program)?;

    let jar_state = Jar::load(jar, program_id)?;
    if vault.address() != &jar_state.vault || source.address() == vault.address() {
        return Err(TipJarError::InvalidVault.into());
    }

    Transfer::new(source,vault, tipper, amount).invoke()
}

/// Accounts: owner (signer), jar, vault (writable), destination token account (writable),
/// token program. Data: amount (u64 LE).
fn withdraw(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [owner, jar, vault, destination, token_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let amount = parse_amount(data)?;
    if !owner.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    check_token_program(token_program)?;

    let jar_state = Jar::load(jar, program_id)?;
    if owner.address() != &jar_state.owner {
        return Err(TipJarError::NotOwner.into());
    }
    if vault.address() != &jar_state.vault {
        return Err(TipJarError::InvalidVault.into());
    }

    let balance = TokenAccount::from_account_view(vault)?.amount();
    if amount > balance {
        return Err(TipJarError::InsufficientBalance.into());
    }

    let bump_seed = [jar_state.bump];
    let seeds = [
        Seed::from(JAR_SEED),
        Seed::from(jar_state.owner.as_ref()),
        Seed::from(jar_state.mint.as_ref()),
        Seed::from(&bump_seed),
    ];
    Transfer::new(vault,destination, jar, amount)
        .invoke_signed(&[Signer::from(&seeds)])
}
