/// Partial Token declarations inlined to avoid an external dependency on the mundis-token-program crate
use mundis_sdk::pubkey::{Pubkey, PUBKEY_BYTES};

mundis_sdk::declare_id!("Token11111111111111111111111111111111111111");

/*
    mundis_token_program::state::Account {
        mint: Pubkey,
        owner: Pubkey,
        amount: u64,
        delegate: Option<Pubkey>,
        state: AccountState,
        is_native: bool,
        delegated_amount: u64,
        close_authority: Option<Pubkey>,
    }
*/
pub const TOKEN_ACCOUNT_MINT_OFFSET: usize = 0;
pub const TOKEN_ACCOUNT_OWNER_OFFSET: usize = 32;
const TOKEN_ACCOUNT_LENGTH: usize = 165;

pub(crate) trait GenericTokenAccount {
    fn valid_account_data(account_data: &[u8]) -> bool;

    // Call after account length has already been verified
    fn unpack_account_owner_unchecked(account_data: &[u8]) -> &Pubkey {
        Self::unpack_pubkey_unchecked(account_data, TOKEN_ACCOUNT_OWNER_OFFSET)
    }

    // Call after account length has already been verified
    fn unpack_account_mint_unchecked(account_data: &[u8]) -> &Pubkey {
        Self::unpack_pubkey_unchecked(account_data, TOKEN_ACCOUNT_MINT_OFFSET)
    }

    // Call after account length has already been verified
    fn unpack_pubkey_unchecked(account_data: &[u8], offset: usize) -> &Pubkey {
        bytemuck::from_bytes(&account_data[offset..offset + PUBKEY_BYTES])
    }

    fn unpack_account_owner(account_data: &[u8]) -> Option<&Pubkey> {
        if Self::valid_account_data(account_data) {
            Some(Self::unpack_account_owner_unchecked(account_data))
        } else {
            None
        }
    }

    fn unpack_account_mint(account_data: &[u8]) -> Option<&Pubkey> {
        if Self::valid_account_data(account_data) {
            Some(Self::unpack_account_mint_unchecked(account_data))
        } else {
            None
        }
    }
}

pub struct Account;
impl Account {
    pub fn get_packed_len() -> usize {
        TOKEN_ACCOUNT_LENGTH
    }
}

impl GenericTokenAccount for Account {
    fn valid_account_data(account_data: &[u8]) -> bool {
        account_data.len() == TOKEN_ACCOUNT_LENGTH
    }
}

pub mod native_mint {
    mundis_sdk::declare_id!("Mun1111111111111111111111111111111111111112");

    /*
        Mint {
            mint_authority: None,
            supply: 0,
            decimals: 9,
            is_initialized: true,
            freeze_authority: None,
            name: "Mundis".to_string(),
            symbol: "MUNDIS".to_string(),
        }
    */
    pub const ACCOUNT_DATA: [u8; 124] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 77, 117, 110, 100, 105, 115, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 77, 85, 78, 68, 73, 83, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 9, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
    ];
}

#[cfg(test)]
pub mod test {
    use mundis_sdk::program_pack::Pack;
    use mundis_token_program::state::Mint;

    #[test]
    fn generate_account_data() {
        let mint = Mint {
            mint_authority: None,
            supply: 0,
            decimals: 9,
            is_initialized: true,
            freeze_authority: None,
            name: "Mundis".to_string(),
            symbol: "MUNDIS".to_string(),
        };

        let mut packed = [0 as u8; Mint::LEN];
        Mint::pack(mint, &mut packed).unwrap();
        println!("{:?}", packed);
    }
}
