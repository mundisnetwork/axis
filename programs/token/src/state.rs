//! State transition types

use arrayref::array_mut_ref;
use num_enum::TryFromPrimitive;
use serde_derive::{Deserialize, Serialize};
use mundis_sdk::instruction::InstructionError;
use mundis_sdk::program_pack::{IsInitialized, Sealed};
use mundis_sdk::pubkey::Pubkey;
use crate::error::TokenError;
use crate::token_instruction::MAX_SIGNERS;

/// Mint data.
#[repr(C)]
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Mint {
    /// Optional authority used to mint new tokens. The mint authority may only be provided during
    /// mint creation. If no mint authority is present then the mint has a fixed supply and no
    /// further tokens may be minted.
    pub mint_authority: Option<Pubkey>,
    /// Total supply of tokens.
    pub supply: u64,
    /// Number of base 10 digits to the right of the decimal place.
    pub decimals: u8,
    /// Is `true` if this structure has been initialized
    pub is_initialized: bool,
    /// Optional authority to freeze token accounts.
    pub freeze_authority: Option<Pubkey>,
}
impl Mint {
    /// The length, in bytes, of the packed representation
    pub const LEN: usize = 82;

    pub fn packed_len() -> usize {
        return Self::LEN;
    }

    pub fn pack(&self, dst: &mut [u8]) -> Result<(), InstructionError> {
        let dst = array_mut_ref![dst, 0, Mint::LEN];
        let serialized_data = bincode::serialize(&self).unwrap();

        assert!(serialized_data.len() <= Self::LEN);

        for (dst, src) in dst.iter_mut().zip(&serialized_data) {
            *dst = *src;
        }
        Ok(())
    }

    pub fn unpack(data: &[u8]) -> Result<Mint, InstructionError> {
        let deser = bincode::deserialize::<Mint>(data);
        if deser.is_err() {
            return Err(TokenError::InvalidState.into());
        }
        return Ok(deser.unwrap());
    }
}
impl Sealed for Mint {}
impl IsInitialized for Mint {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

/// Account data.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Account {
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
    /// MUN accounts do not drop below this threshold.
    pub is_native: bool,
    /// The amount delegated
    pub delegated_amount: u64,
    /// Optional authority to close the account.
    pub close_authority: Option<Pubkey>,
}
impl Account {
    /// The length, in bytes, of the packed representation
    pub const LEN: usize = 165;

    /// Checks if account is frozen
    pub fn is_frozen(&self) -> bool {
        self.state == AccountState::Frozen
    }
    /// Checks if account is native
    pub fn is_native(&self) -> bool {
        self.is_native
    }

    pub fn packed_len() -> usize {
        return Self::LEN;
    }

    pub fn pack(&self, dst: &mut [u8]) -> Result<(), InstructionError> {
        let dst = array_mut_ref![dst, 0, Account::LEN];
        let serialized_data = bincode::serialize(&self)
            .unwrap();

        assert!(serialized_data.len() <= Self::LEN);
        for (dst, src) in dst.iter_mut().zip(&serialized_data) {
            *dst = *src;
        }
        Ok(())
    }

    pub fn unpack(data: &[u8]) -> Result<Account, InstructionError> {
        let deser = bincode::deserialize::<Account>(data);
        if deser.is_err() {
            return Err(TokenError::InvalidState.into());
        }
        return Ok(deser.unwrap());
    }
}
impl Sealed for Account {}
impl IsInitialized for Account {
    fn is_initialized(&self) -> bool {
        self.state != AccountState::Uninitialized
    }
}

/// Account state.
#[repr(u8)]
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, TryFromPrimitive)]
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
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
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

impl Multisig {
    /// The length, in bytes, of the packed representation
    const LEN: usize = 355;

    pub fn packed_len() -> usize {
        return Self::LEN;
    }

    pub fn pack(&self, dst: &mut [u8]) -> Result<(), InstructionError> {
        let dst = array_mut_ref![dst, 0, Multisig::LEN];
        let serialized_data = bincode::serialize(&self)
            .unwrap();

        assert!(serialized_data.len() <= Self::LEN);
        for (dst, src) in dst.iter_mut().zip(&serialized_data) {
            *dst = *src;
        }
        Ok(())
    }

    pub fn unpack(data: &[u8]) -> Result<Multisig, InstructionError> {
        let deser = bincode::deserialize::<Multisig>(data);
        if deser.is_err() {
            return Err(TokenError::InvalidState.into());
        }
        return Ok(deser.unwrap());
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
    use mundis_program::pubkey::Pubkey;
    use mundis_sdk::pubkey::Pubkey;
    use crate::state::{Account, AccountState, Mint, Multisig};
    use crate::token_instruction::MAX_SIGNERS;

    #[test]
    fn test_pack_unpack() {
        // Mint
        let check = Mint {
            mint_authority: Some(Pubkey::new(&[1; 32])),
            supply: 42,
            decimals: 7,
            is_initialized: true,
            freeze_authority: Some(Pubkey::new(&[2; 32])),
        };
        let packed_len = bincode::serialized_size(&check).unwrap();
        assert!(packed_len <= Mint::packed_len() as u64);

        let packed = bincode::serialize(&check).unwrap();
        assert_eq!(check, bincode::deserialize::<Mint>(&packed).unwrap());

        // Account
        let check = Account {
            mint: Pubkey::new(&[1; 32]),
            owner: Pubkey::new(&[2; 32]),
            amount: 3,
            delegate: Some(Pubkey::new(&[4; 32])),
            state: AccountState::Frozen,
            is_native: true,
            delegated_amount: 6,
            close_authority: Some(Pubkey::new(&[7; 32])),
        };
        let packed_len = bincode::serialized_size(&check).unwrap();
        assert!(packed_len <= Account::packed_len() as u64);

        let packed = bincode::serialize(&check).unwrap();
        assert_eq!(check, bincode::deserialize::<Account>(&packed).unwrap());

        // Multisig
        let check = Multisig {
            m: 1,
            n: 2,
            is_initialized: true,
            signers: [Pubkey::new(&[3; 32]); MAX_SIGNERS],
        };
        let packed_len = bincode::serialized_size(&check).unwrap();
        assert!(packed_len <= Multisig::packed_len() as u64);

        let packed = bincode::serialize(&check).unwrap();
        assert_eq!(check, bincode::deserialize::<Multisig>(&packed).unwrap());
    }
}

