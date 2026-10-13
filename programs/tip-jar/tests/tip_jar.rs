use {
    mollusk_svm::{program::keyed_account_for_system_program, result::InstructionResult, Mollusk},
    mollusk_svm_programs_token::token,
    solana_account::Account,
    solana_instruction::{AccountMeta, Instruction},
    solana_program_error::ProgramError,
    solana_pubkey::Pubkey,
};

const SYSTEM_PROGRAM: Pubkey = Pubkey::new_from_array([0; 32]);
const MINT_LEN: usize = 82;
const TOKEN_ACCOUNT_LEN: usize = 165;
const INITIAL_TIPPER_BALANCE: u64 = 1_000;

const ERR_ZERO_AMOUNT: u32 = 0;
const ERR_INSUFFICIENT_BALANCE: u32 = 1;
const ERR_NOT_OWNER: u32 = 2;
const ERR_INVALID_JAR: u32 = 3;
const ERR_INVALID_VAULT: u32 = 4;

fn mint_account() -> Account {
    let mut data = vec![0u8; MINT_LEN];
    data[44] = 6; // decimals
    data[45] = 1; // is_initialized
    Account {
        lamports: 10_000_000,
        data,
        owner: token::ID,
        executable: false,
        rent_epoch: 0,
    }
}

fn token_account(mint: &Pubkey, owner: &Pubkey, amount: u64) -> Account {
    let mut data = vec![0u8; TOKEN_ACCOUNT_LEN];
    data[0..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data[108] = 1; // initialized
    Account {
        lamports: 10_000_000,
        data,
        owner: token::ID,
        executable: false,
        rent_epoch: 0,
    }
}

fn token_amount(account: &Account) -> u64 {
    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

fn wallet(lamports: u64) -> Account {
    Account::new(lamports, 0, &SYSTEM_PROGRAM)
}

struct Env {
    mollusk: Mollusk,
    program_id: Pubkey,
    payer: Pubkey,
    owner: Pubkey,
    stranger: Pubkey,
    tipper: Pubkey,
    tipper_tokens: Pubkey,
    owner_tokens: Pubkey,
    stranger_tokens: Pubkey,
    mint: Pubkey,
    jar: Pubkey,
    vault: Pubkey,
    accounts: Vec<(Pubkey, Account)>,
}

impl Env {
    fn new() -> Self {
        if std::env::var_os("SBF_OUT_DIR").is_none() {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/deploy");
            std::env::set_var("SBF_OUT_DIR", dir);
        }

        let program_id = Pubkey::new_unique();
        let mut mollusk = Mollusk::new(&program_id, "tip_jar");
        token::add_program(&mut mollusk);

        let payer = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let stranger = Pubkey::new_unique();
        let tipper = Pubkey::new_unique();
        let tipper_tokens = Pubkey::new_unique();
        let owner_tokens = Pubkey::new_unique();
        let stranger_tokens = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let (jar, _) =
            Pubkey::find_program_address(&[b"jar", owner.as_ref(), mint.as_ref()], &program_id);
        let (vault, _) = Pubkey::find_program_address(&[b"vault", jar.as_ref()], &program_id);

        let accounts = vec![
            (payer, wallet(10_000_000_000)),
            (owner, wallet(1_000_000_000)),
            (stranger, wallet(1_000_000_000)),
            (tipper, wallet(1_000_000_000)),
            (mint, mint_account()),
            (
                tipper_tokens,
                token_account(&mint, &tipper, INITIAL_TIPPER_BALANCE),
            ),
            (owner_tokens, token_account(&mint, &owner, 0)),
            (stranger_tokens, token_account(&mint, &stranger, 0)),
            (jar, wallet(0)),
            (vault, wallet(0)),
            keyed_account_for_system_program(),
            token::keyed_account(),
        ];

        Self {
            mollusk,
            program_id,
            payer,
            owner,
            stranger,
            tipper,
            tipper_tokens,
            owner_tokens,
            stranger_tokens,
            mint,
            jar,
            vault,
            accounts,
        }
    }

    fn ix(&self, data: Vec<u8>, metas: Vec<AccountMeta>) -> Instruction {
        Instruction::new_with_bytes(self.program_id, &data, metas)
    }

    fn initialize(&self) -> Instruction {
        let mut data = vec![0u8];
        data.extend_from_slice(self.owner.as_ref());
        self.ix(
            data,
            vec![
                AccountMeta::new(self.payer, true),
                AccountMeta::new(self.jar, false),
                AccountMeta::new(self.vault, false),
                AccountMeta::new_readonly(self.mint, false),
                AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
                AccountMeta::new_readonly(token::ID, false),
            ],
        )
    }

    fn tip(&self, amount: u64) -> Instruction {
        let mut data = vec![1u8];
        data.extend_from_slice(&amount.to_le_bytes());
        self.ix(
            data,
            vec![
                AccountMeta::new_readonly(self.tipper, true),
                AccountMeta::new(self.tipper_tokens, false),
                AccountMeta::new_readonly(self.jar, false),
                AccountMeta::new(self.vault, false),
                AccountMeta::new_readonly(token::ID, false),
            ],
        )
    }

    fn withdraw(&self, authority: Pubkey, destination: Pubkey, amount: u64) -> Instruction {
        let mut data = vec![2u8];
        data.extend_from_slice(&amount.to_le_bytes());
        self.ix(
            data,
            vec![
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(self.jar, false),
                AccountMeta::new(self.vault, false),
                AccountMeta::new(destination, false),
                AccountMeta::new_readonly(token::ID, false),
            ],
        )
    }

    fn run(&self, ixs: &[Instruction]) -> InstructionResult {
        self.mollusk.process_instruction_chain(ixs, &self.accounts)
    }
}

fn assert_custom_error(result: &InstructionResult, code: u32) {
    assert_eq!(
        result.program_result,
        mollusk_svm::result::ProgramResult::Failure(ProgramError::Custom(code)),
        "unexpected result: {:?}",
        result.program_result
    );
}

fn balance_of(result: &InstructionResult, key: &Pubkey) -> u64 {
    token_amount(result.get_account(key).expect("account missing"))
}

#[test]
fn tip_then_withdraw_works() {
    let env = Env::new();
    let result = env.run(&[
        env.initialize(),
        env.tip(600),
        env.withdraw(env.owner, env.owner_tokens, 250),
    ]);
    assert!(result.program_result.is_ok(), "{:?}", result.program_result);

    assert_eq!(balance_of(&result, &env.tipper_tokens), 400);
    assert_eq!(balance_of(&result, &env.vault), 350);
    assert_eq!(balance_of(&result, &env.owner_tokens), 250);

    let jar = result.get_account(&env.jar).unwrap();
    assert_eq!(jar.owner, env.program_id);
    assert_eq!(&jar.data[2..34], env.owner.as_ref());
    assert_eq!(&jar.data[34..66], env.mint.as_ref());
    assert_eq!(&jar.data[66..98], env.vault.as_ref());
}

#[test]
fn withdraw_full_balance_works() {
    let env = Env::new();
    let result = env.run(&[
        env.initialize(),
        env.tip(600),
        env.withdraw(env.owner, env.owner_tokens, 600),
    ]);
    assert!(result.program_result.is_ok(), "{:?}", result.program_result);
    assert_eq!(balance_of(&result, &env.vault), 0);
    assert_eq!(balance_of(&result, &env.owner_tokens), 600);
}

#[test]
fn zero_tip_fails() {
    let env = Env::new();
    let result = env.run(&[env.initialize(), env.tip(0)]);
    assert_custom_error(&result, ERR_ZERO_AMOUNT);
}

#[test]
fn stranger_cannot_withdraw() {
    let env = Env::new();
    let result = env.run(&[
        env.initialize(),
        env.tip(500),
        env.withdraw(env.stranger, env.stranger_tokens, 100),
    ]);
    assert_custom_error(&result, ERR_NOT_OWNER);
}

#[test]
fn owner_without_signature_cannot_withdraw() {
    let env = Env::new();
    let mut withdraw = env.withdraw(env.owner, env.stranger_tokens, 100);
    withdraw.accounts[0].is_signer = false;
    let result = env.run(&[env.initialize(), env.tip(500), withdraw]);
    assert_eq!(
        result.program_result,
        mollusk_svm::result::ProgramResult::Failure(ProgramError::MissingRequiredSignature)
    );
}

#[test]
fn withdraw_more_than_balance_fails() {
    let env = Env::new();
    let result = env.run(&[
        env.initialize(),
        env.tip(500),
        env.withdraw(env.owner, env.owner_tokens, 501),
    ]);
    assert_custom_error(&result, ERR_INSUFFICIENT_BALANCE);
}

#[test]
fn withdraw_from_empty_jar_fails() {
    let env = Env::new();
    let result = env.run(&[
        env.initialize(),
        env.withdraw(env.owner, env.owner_tokens, 1),
    ]);
    assert_custom_error(&result, ERR_INSUFFICIENT_BALANCE);
}

#[test]
fn tip_more_than_sender_holds_fails() {
    let env = Env::new();
    let result = env.run(&[env.initialize(), env.tip(INITIAL_TIPPER_BALANCE + 1)]);
    assert!(result.program_result.is_err());
}

#[test]
fn tip_to_wrong_vault_fails() {
    let env = Env::new();
    let mut tip = env.tip(10);
    tip.accounts[3] = AccountMeta::new(env.owner_tokens, false);
    let result = env.run(&[env.initialize(), tip]);
    assert_custom_error(&result, ERR_INVALID_VAULT);
}

#[test]
fn withdraw_to_wrong_vault_fails() {
    let env = Env::new();
    let mut withdraw = env.withdraw(env.owner, env.owner_tokens, 10);
    withdraw.accounts[2] = AccountMeta::new(env.tipper_tokens, false);
    let result = env.run(&[env.initialize(), env.tip(500), withdraw]);
    assert_custom_error(&result, ERR_INVALID_VAULT);
}

#[test]
fn fake_jar_is_rejected() {
    let env = Env::new();
    let mut tip = env.tip(10);
    tip.accounts[2] = AccountMeta::new_readonly(env.owner_tokens, false);
    let result = env.run(&[env.initialize(), tip]);
    assert_custom_error(&result, ERR_INVALID_JAR);
}

#[test]
fn initialize_twice_fails() {
    let env = Env::new();
    let result = env.run(&[env.initialize(), env.initialize()]);
    assert!(result.program_result.is_err());
}

#[test]
fn initialize_with_wrong_jar_address_fails() {
    let env = Env::new();
    let mut init = env.initialize();
    init.accounts[1] = AccountMeta::new(env.stranger, false);
    let result = env.run(&[init]);
    assert_eq!(
        result.program_result,
        mollusk_svm::result::ProgramResult::Failure(ProgramError::InvalidSeeds)
    );
}

#[test]
fn initialize_succeeds_when_pdas_are_prefunded() {
    let mut env = Env::new();
    for (key, account) in env.accounts.iter_mut() {
        if *key == env.jar || *key == env.vault {
            account.lamports = 1;
        }
    }
    let result = env.run(&[
        env.initialize(),
        env.tip(100),
        env.withdraw(env.owner, env.owner_tokens, 100),
    ]);
    assert!(result.program_result.is_ok(), "{:?}", result.program_result);
    assert_eq!(balance_of(&result, &env.owner_tokens), 100);
}

#[test]
fn tip_requires_tipper_signature() {
    let env = Env::new();
    let mut tip = env.tip(10);
    tip.accounts[0].is_signer = false;
    let result = env.run(&[env.initialize(), tip]);
    assert_eq!(
        result.program_result,
        mollusk_svm::result::ProgramResult::Failure(ProgramError::MissingRequiredSignature)
    );
}
