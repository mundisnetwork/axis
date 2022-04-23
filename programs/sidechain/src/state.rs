use arrayref::array_mut_ref;
use serde_derive::{Deserialize, Serialize};
use mundis_sdk::instruction::InstructionError;
use mundis_sdk::program_pack::{IsInitialized, Sealed};

use mundis_sdk::pubkey::Pubkey;
use crate::error::SidechainError;

#[repr(C)]
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Default, Clone)]
pub struct SidechainRecord {
    pub chain_owner: Pubkey,
    pub website_url: Option<String>,
    pub github_url: Option<String>,
    pub contact_email: Option<String>,
    pub deposit: u64,
    pub state: SidechainState,
    pub vote_deposit: u64,
    pub registration_time: u64,
    pub boot_time: u64,
    pub validator_count: u16,
    pub total_stake: u64,
    /// Is `true` if this structure has been initialized
    pub is_initialized: bool,
}
impl Sealed for SidechainRecord {}
impl IsInitialized for SidechainRecord {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}
impl SidechainRecord {
    /// The length, in bytes, of the packed representation
    pub const LEN: usize = 426;

    pub fn packed_len() -> usize {
        return Self::LEN;
    }

    pub fn pack(&self, dst: &mut [u8]) -> Result<(), InstructionError> {
        let dst = array_mut_ref![dst, 0, SidechainRecord::LEN];
        let serialized_data = bincode::serialize(&self).unwrap();
        assert!(serialized_data.len() <= SidechainRecord::LEN);
        for (dst, src) in dst.iter_mut().zip(&serialized_data) {
            *dst = *src;
        }
        Ok(())
    }

    pub fn unpack(data: &[u8]) -> Result<Self, InstructionError> {
        assert!(data.len() <= SidechainRecord::LEN);
        let deser = bincode::deserialize::<Self>(data);
        if deser.is_err() {
            return Err(SidechainError::InvalidState.into());
        }
        return Ok(deser.unwrap());
    }
}

#[repr(u8)]
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum SidechainState {
    Uninitialized,
    Registered,
    InQueue,
    Staging,
    Booting,
    Active,
    Broken,
    Dead,
}
impl Default for SidechainState {
    fn default() -> Self {
        SidechainState::Uninitialized
    }
}

#[cfg(test)]
mod test {
    use std::mem;
    use std::str::from_utf8;
    use mundis_sdk::instruction::Instruction;
    use mundis_sdk::native_token::mun_to_lamports;
    use crate::Pubkey;
    use crate::state::{SidechainRecord, SidechainState};

    #[test]
    fn print_max_length() {
        let max_url = from_utf8(&['a' as u8;128]).unwrap();
        let max_email = from_utf8(&['a' as u8;64]).unwrap();

        let rec1 = SidechainRecord {
            chain_owner: Pubkey::new_unique(),
            website_url: Some(String::from(max_url)),
            github_url: Some(String::from(max_url)),
            contact_email: Some(String::from(max_email)),
            deposit: 0,
            state: SidechainState::Registered,
            vote_deposit: 0,
            registration_time: 0,
            boot_time: 0,
            validator_count: 0,
            total_stake: 0,
            is_initialized: false
        };

        let serialized_data = bincode::serialize(&rec1).unwrap();
        println!("sizeof={}",serialized_data.len());
    }
}