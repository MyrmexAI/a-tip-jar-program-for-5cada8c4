use pinocchio::error::ProgramError;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipJarError {
    ZeroAmount = 0,
    InsufficientBalance = 1,
    NotOwner = 2,
    InvalidJar = 3,
    InvalidVault = 4,
}

impl From<TipJarError> for ProgramError {
    fn from(e: TipJarError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
