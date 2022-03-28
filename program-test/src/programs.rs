use mundis_sdk::{
    account::{Account, AccountSharedData},
    pubkey::Pubkey,
    rent::Rent,
};

mod anima_token {
    mundis_sdk::declare_id!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
}
mod anima_memo {
    mundis_sdk::declare_id!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
}
mod anima_token_account {
    mundis_sdk::declare_id!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
}

static ANIMA_PROGRAMS: &[(Pubkey, &[u8])] = &[
    (anima_token::ID, include_bytes!("programs/anima_token-3.2.0.so")),
    (
        anima_memo::ID,
        include_bytes!("programs/anima_memo-3.0.0.so"),
    ),
    (
        anima_token_account::ID,
        include_bytes!("programs/anima_token-account-1.0.3.so"),
    ),
];

pub fn anima_programs(rent: &Rent) -> Vec<(Pubkey, AccountSharedData)> {
    ANIMA_PROGRAMS
        .iter()
        .map(|(program_id, elf)| {
            (
                *program_id,
                AccountSharedData::from(Account {
                    lamports: rent.minimum_balance(elf.len()).min(1),
                    data: elf.to_vec(),
                    owner: mundis_sdk::bpf_loader::id(),
                    executable: true,
                    rent_epoch: 0,
                }),
            )
        })
        .collect()
}
