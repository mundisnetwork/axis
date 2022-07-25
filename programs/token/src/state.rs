//! State transition types

use arrayref::{array_mut_ref, array_ref, array_refs, mut_array_refs};
use num_enum::TryFromPrimitive;
use mundis_sdk::instruction::InstructionError;
use mundis_sdk::program_pack::{IsInitialized, Pack, Sealed};
use mundis_sdk::pubkey::Pubkey;
use crate::token_instruction::MAX_SIGNERS;

pub const MAX_NAME_LENGTH: usize = 32;
pub const MAX_SYMBOL_LENGTH: usize = 10;

/// Mint data.
#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mint {
    /// Optional authority used to mint new tokens. The mint authority may only be provided during
    /// mint creation. If no mint authority is present then the mint has a fixed supply and no
    /// further tokens may be minted.
    pub mint_authority: Option<Pubkey>,
    /// The name of the asset
    pub name: String,
    /// The symbol for the asset
    pub symbol: String,
    /// Total supply of tokens.
    pub supply: u64,
    /// Number of base 10 digits to the right of the decimal place.
    pub decimals: u8,
    /// Is `true` if this structure has been initialized
    pub is_initialized: bool,
    /// Optional authority to freeze token accounts.
    pub freeze_authority: Option<Pubkey>,
}

impl Sealed for Mint {}

impl IsInitialized for Mint {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

impl Pack for Mint {
    /// The length, in bytes, of the packed representation
    const LEN: usize = 124;

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let dst = array_mut_ref![dst, 0, 124];
        let (
            mint_authority_dst,
            name_dst,
            symbol_dst,
            supply_dst,
            decimals_dst,
            is_initialized_dst,
            freeze_authority_dst,
        ) = mut_array_refs![dst, 36, 32, 10, 8, 1, 1, 36];
        let &Mint {
            ref mint_authority,
            ref name,
            ref symbol,
            supply,
            decimals,
            is_initialized,
            ref freeze_authority,
        } = self;
        pack_option_key(mint_authority, mint_authority_dst);

        let name_padded = puffed_out_string(name, MAX_NAME_LENGTH);
        name_dst.copy_from_slice(name_padded.as_ref());

        let symbol_padded = puffed_out_string(symbol, MAX_SYMBOL_LENGTH);
        symbol_dst.copy_from_slice(symbol_padded.as_ref());

        *supply_dst = supply.to_le_bytes();
        decimals_dst[0] = decimals;
        is_initialized_dst[0] = is_initialized as u8;
        pack_option_key(freeze_authority, freeze_authority_dst);
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, InstructionError> {
        let src = array_ref![src, 0, 124];
        let (mint_authority, name, symbol, supply, decimals, is_initialized, freeze_authority) =
            array_refs![src, 36, 32, 10, 8, 1, 1, 36];
        let mint_authority = unpack_option_key(mint_authority)?;
        let name = String::from_utf8_lossy(name).to_string();
        let symbol = String::from_utf8_lossy(symbol).to_string();
        let supply = u64::from_le_bytes(*supply);
        let decimals = decimals[0];
        let is_initialized = match is_initialized {
            [0] => false,
            [1] => true,
            _ => return Err(InstructionError::InvalidAccountData),
        };
        let freeze_authority = unpack_option_key(freeze_authority)?;
        Ok(Mint {
            mint_authority,
            name,
            symbol,
            supply,
            decimals,
            is_initialized,
            freeze_authority,
        })
    }
}

/// Account data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenAccount {
    /// The mint associated with this account
    pub mint: Pubkey,
    /// The owner of this account.
    pub owner: Pubkey,
    /// The amount of tokens this account holds.
    pub amount: u64,
    /// If `delegate` is `Some` then `delegated_amount` represents
    /// the amount authorized by the delegate
    pub delegate: Option<Pubkey>,
    /// The account's state
    pub state: AccountState,
    /// If is_some, this is a native token, and the value logs the rent-exempt reserve. An Account
    /// is required to be rent-exempt, so the value is used by the Processor to ensure that wrapped
    /// MUNDIS accounts do not drop below this threshold.
    pub is_native: Option<u64>,
    /// The amount delegated
    pub delegated_amount: u64,
    /// Optional authority to close the account.
    pub close_authority: Option<Pubkey>,
}

impl TokenAccount {
    /// Checks if account is frozen
    pub fn is_frozen(&self) -> bool {
        self.state == AccountState::Frozen
    }
    /// Checks if account is native
    pub fn is_native(&self) -> bool {
        self.is_native.is_some()
    }
}

impl Sealed for TokenAccount {}

impl IsInitialized for TokenAccount {
    fn is_initialized(&self) -> bool {
        self.state != AccountState::Uninitialized
    }
}

impl Pack for TokenAccount {
    const LEN: usize = 165;

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let dst = array_mut_ref![dst, 0, 165];
        let (
            mint_dst,
            owner_dst,
            amount_dst,
            delegate_dst,
            state_dst,
            is_native_dst,
            delegated_amount_dst,
            close_authority_dst,
        ) = mut_array_refs![dst, 32, 32, 8, 36, 1, 12, 8, 36];
        let &TokenAccount {
            ref mint,
            ref owner,
            amount,
            ref delegate,
            state,
            ref is_native,
            delegated_amount,
            ref close_authority,
        } = self;
        mint_dst.copy_from_slice(mint.as_ref());
        owner_dst.copy_from_slice(owner.as_ref());
        *amount_dst = amount.to_le_bytes();
        pack_option_key(delegate, delegate_dst);
        state_dst[0] = state as u8;
        pack_option_u64(is_native, is_native_dst);
        *delegated_amount_dst = delegated_amount.to_le_bytes();
        pack_option_key(close_authority, close_authority_dst);
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, InstructionError> {
        let src = array_ref![src, 0, 165];
        let (mint, owner, amount, delegate, state, is_native, delegated_amount, close_authority) =
            array_refs![src, 32, 32, 8, 36, 1, 12, 8, 36];
        Ok(TokenAccount {
            mint: Pubkey::new_from_array(*mint),
            owner: Pubkey::new_from_array(*owner),
            amount: u64::from_le_bytes(*amount),
            delegate: unpack_option_key(delegate)?,
            state: AccountState::try_from_primitive(state[0])
                .or(Err(InstructionError::InvalidAccountData))?,
            is_native: unpack_option_u64(is_native)?,
            delegated_amount: u64::from_le_bytes(*delegated_amount),
            close_authority: unpack_option_key(close_authority)?,
        })
    }
}

/// Account state.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, TryFromPrimitive)]
pub enum AccountState {
    /// Account is not yet initialized
    Uninitialized,
    /// Account is initialized; the account owner and/or delegate may perform permitted operations
    /// on this account
    Initialized,
    /// Account has been frozen by the mint freeze authority. Neither the account owner nor
    /// the delegate are able to perform operations on this account.
    Frozen,
}

impl Default for AccountState {
    fn default() -> Self {
        AccountState::Uninitialized
    }
}

/// Multisignature data.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Multisig {
    /// Number of signers required
    pub m: u8,
    /// Number of valid signers
    pub n: u8,
    /// Is `true` if this structure has been initialized
    pub is_initialized: bool,
    /// Signer public keys
    pub signers: [Pubkey; MAX_SIGNERS],
}

impl Pack for Multisig {
    const LEN: usize = 355;

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let dst = array_mut_ref![dst, 0, 355];
        #[allow(clippy::ptr_offset_with_cast)]
            let (m, n, is_initialized, signers_flat) = mut_array_refs![dst, 1, 1, 1, 32 * MAX_SIGNERS];
        *m = [self.m];
        *n = [self.n];
        *is_initialized = [self.is_initialized as u8];
        for (i, src) in self.signers.iter().enumerate() {
            let dst_array = array_mut_ref![signers_flat, 32 * i, 32];
            dst_array.copy_from_slice(src.as_ref());
        }
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, InstructionError> {
        let src = array_ref![src, 0, 355];
        #[allow(clippy::ptr_offset_with_cast)]
            let (m, n, is_initialized, signers_flat) = array_refs![src, 1, 1, 1, 32 * MAX_SIGNERS];
        let mut result = Multisig {
            m: m[0],
            n: n[0],
            is_initialized: match is_initialized {
                [0] => false,
                [1] => true,
                _ => return Err(InstructionError::InvalidAccountData),
            },
            signers: [Pubkey::new_from_array([0u8; 32]); MAX_SIGNERS],
        };
        for (src, dst) in signers_flat.chunks(32).zip(result.signers.iter_mut()) {
            *dst = Pubkey::new(src);
        }
        Ok(result)
    }
}


impl Sealed for Multisig {}

impl IsInitialized for Multisig {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

#[cfg(test)]
mod tests {
    use mundis_sdk::program_pack::Pack;
    use mundis_sdk::pubkey::Pubkey;
    use crate::InstructionError;
    use crate::state::{TokenAccount, AccountState, Mint, Multisig, unpack_option_u64, unpack_option_key};

    #[test]
    fn test_mint_unpack_from_slice() {
        let src: [u8; 124] = [0; 124];
        let mint = Mint::unpack_from_slice(&src).unwrap();
        assert!(!mint.is_initialized);

        let mut src: [u8; 124] = [0; 124];
        src[0] = 2;
        let mint = Mint::unpack_from_slice(&src).unwrap_err();
        assert_eq!(mint, InstructionError::InvalidAccountData);
    }

    #[test]
    fn test_account_state() {
        let account_state = AccountState::default();
        assert_eq!(account_state, AccountState::Uninitialized);
    }

    #[test]
    fn test_multisig_unpack_from_slice() {
        let src: [u8; 355] = [0; 355];
        let multisig = Multisig::unpack_from_slice(&src).unwrap();
        assert_eq!(multisig.m, 0);
        assert_eq!(multisig.n, 0);
        assert!(!multisig.is_initialized);

        let mut src: [u8; 355] = [0; 355];
        src[0] = 1;
        src[1] = 1;
        src[2] = 1;
        let multisig = Multisig::unpack_from_slice(&src).unwrap();
        assert_eq!(multisig.m, 1);
        assert_eq!(multisig.n, 1);
        assert!(multisig.is_initialized);

        let mut src: [u8; 355] = [0; 355];
        src[2] = 2;
        let multisig = Multisig::unpack_from_slice(&src).unwrap_err();
        assert_eq!(multisig, InstructionError::InvalidAccountData);
    }

    #[test]
    fn test_unpack_option_key() {
        let src: [u8; 36] = [0; 36];
        let result = unpack_option_key(&src).unwrap();
        assert_eq!(result, None);

        let mut src: [u8; 36] = [0; 36];
        src[1] = 1;
        let result = unpack_option_key(&src).unwrap_err();
        assert_eq!(result, InstructionError::InvalidAccountData);
    }

    #[test]
    fn test_unpack_option_u64() {
        let src: [u8; 12] = [0; 12];
        let result = unpack_option_u64(&src).unwrap();
        assert_eq!(result, None);

        let mut src: [u8; 12] = [0; 12];
        src[0] = 1;
        let result = unpack_option_u64(&src).unwrap();
        assert_eq!(result, Some(0));

        let mut src: [u8; 12] = [0; 12];
        src[1] = 1;
        let result = unpack_option_u64(&src).unwrap_err();
        assert_eq!(result, InstructionError::InvalidAccountData);
    }

    #[test]
    fn test_pack_unpack_token_account() {
        let mint = Pubkey::new(&[2; 32]);
        let owner = Pubkey::new(&[3; 32]);
        let delegate = Pubkey::new(&[4; 32]);

        let mut dst = [0; TokenAccount::LEN];
        let token_account = TokenAccount {
            mint,
            owner,
            delegate: Some(delegate),
            amount: 420,
            state: AccountState::Initialized,
            is_native: None,
            delegated_amount: 30,
            close_authority: Some(owner),
        };
        TokenAccount::pack(token_account.clone(), &mut dst).unwrap();

        let unpacked = TokenAccount::unpack(&dst).unwrap();
        assert_eq!(token_account, unpacked);
    }
}

// Helpers
fn pack_option_key(src: &Option<Pubkey>, dst: &mut [u8; 36]) {
    let (tag, body) = mut_array_refs![dst, 4, 32];
    match src {
        Some(key) => {
            *tag = [1, 0, 0, 0];
            body.copy_from_slice(key.as_ref());
        }
        None => {
            *tag = [0; 4];
        }
    }
}

fn unpack_option_key(src: &[u8; 36]) -> Result<Option<Pubkey>, InstructionError> {
    let (tag, body) = array_refs![src, 4, 32];
    match *tag {
        [0, 0, 0, 0] => Ok(None),
        [1, 0, 0, 0] => Ok(Some(Pubkey::new_from_array(*body))),
        _ => Err(InstructionError::InvalidAccountData),
    }
}

fn pack_option_u64(src: &Option<u64>, dst: &mut [u8; 12]) {
    let (tag, body) = mut_array_refs![dst, 4, 8];
    match src {
        Some(amount) => {
            *tag = [1, 0, 0, 0];
            *body = amount.to_le_bytes();
        }
        None => {
            *tag = [0; 4];
        }
    }
}
fn unpack_option_u64(src: &[u8; 12]) -> Result<Option<u64>, InstructionError> {
    let (tag, body) = array_refs![src, 4, 8];
    match *tag {
        [0, 0, 0, 0] => Ok(None),
        [1, 0, 0, 0] => Ok(Some(u64::from_le_bytes(*body))),
        _ => Err(InstructionError::InvalidAccountData),
    }
}

/// Pads the string to the desired size with `0u8`s.
/// NOTE: it is assumed that the string's size is never larger than the given size.
pub fn puffed_out_string(s: &String, size: usize) -> String {
    let mut array_of_zeroes = vec![];
    let puff_amount = size - s.len();
    while array_of_zeroes.len() < puff_amount {
        array_of_zeroes.push(0u8);
    }
    s.clone() + std::str::from_utf8(&array_of_zeroes).unwrap()
}

#[cfg(test)]
mod test {
    use mundis_sdk::program_pack::Pack;
    use crate::Pubkey;
    use crate::state::Mint;

    #[test]
    fn test_pack_unpack() {
        let mint_authority = Pubkey::new_unique();
        let freeze_authority = Pubkey::new_unique();

        let mint0 = Mint {
            mint_authority: Some(mint_authority),
            name: "ArchitectZeroToken".to_string(),
            symbol: "AZT".to_string(),
            supply: 4242,
            decimals: 2,
            is_initialized: true,
            freeze_authority: Some(freeze_authority),
        };

        let mut buf = [0 as u8; Mint::LEN];
        mint0.pack_into_slice(&mut buf);

        let mint1 = Mint::unpack_from_slice(&buf)
            .unwrap();

        assert_eq!(mint0.mint_authority, mint1.mint_authority);
        assert_eq!(mint1.name.as_str(), "ArchitectZeroToken\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}");
        assert_eq!(mint1.symbol.as_str(), "AZT\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}");
        assert_eq!(mint0.supply, mint1.supply);
        assert_eq!(mint0.decimals, mint1.decimals);
        assert_eq!(mint0.is_initialized, mint1.is_initialized);
        assert_eq!(mint0.freeze_authority, mint1.freeze_authority);
    }
}

