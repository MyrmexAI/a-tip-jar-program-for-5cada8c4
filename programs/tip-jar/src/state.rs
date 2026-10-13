use pinocchio::{error::ProgramError, AccountView, Address};

use crate::error::TipJarError;

pub const JAR_SEED: &[u8] = b"jar";
pub const VAULT_SEED: &[u8] = b"vault";

pub const JAR_DISCRIMINATOR: u8 = 1;

/// Jar account layout: `[discriminator: u8][bump: u8][owner: 32][mint: 32][vault: 32]`.
pub const JAR_LEN: usize = 2 + 32 * 3;

const BUMP_OFFSET: usize = 1;
const OWNER_OFFSET: usize = 2;
const MINT_OFFSET: usize = OWNER_OFFSET + 32;
const VAULT_OFFSET: usize = MINT_OFFSET + 32;

pub struct Jar {
    pub bump: u8,
    pub owner: Address,
    pub mint: Address,
    pub vault: Address,
}

impl Jar {
    /// Loads a jar owned by `program_id`. Only this program can create accounts with this
    /// owner and layout, and `initialize` is the only place that writes the discriminator.
    pub fn load(account: &AccountView, program_id: &Address) -> Result<Self, ProgramError> {
        if !account.owned_by(program_id) || account.data_len() != JAR_LEN {
            return Err(TipJarError::InvalidJar.into());
        }
        let data = account.try_borrow()?;
        if data[0] != JAR_DISCRIMINATOR {
            return Err(TipJarError::InvalidJar.into());
        }
        Ok(Self {
            bump: data[BUMP_OFFSET],
            owner: read_address(&data, OWNER_OFFSET),
            mint: read_address(&data, MINT_OFFSET),
            vault: read_address(&data, VAULT_OFFSET),
        })
    }

    pub fn store(&self, account: &mut AccountView) -> Result<(), ProgramError> {
        let mut data = account.try_borrow_mut()?;
        data[0] = JAR_DISCRIMINATOR;
        data[BUMP_OFFSET] = self.bump;
        data[OWNER_OFFSET..MINT_OFFSET].copy_from_slice(self.owner.as_ref());
        data[MINT_OFFSET..VAULT_OFFSET].copy_from_slice(self.mint.as_ref());
        data[VAULT_OFFSET..JAR_LEN].copy_from_slice(self.vault.as_ref());
        Ok(())
    }
}

fn read_address(data: &[u8], offset: usize) -> Address {
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&data[offset..offset + 32]);
    Address::from(bytes)
}
