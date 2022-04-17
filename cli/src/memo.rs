use std::sync::Arc;
use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};
use {
    mundis_sdk::{instruction::Instruction, pubkey::Pubkey},
};
use mundis_clap_utils::keypair::DefaultSigner;
use mundis_client::rpc_client::RpcClient;
use mundis_remote_wallet::remote_wallet::RemoteWalletManager;
use mundis_sdk::message::Message;
use mundis_sdk::transaction::Transaction;
use crate::cli::{CliCommand, CliCommandInfo, CliConfig, CliError, ProcessResult};
use crate::spend_utils::{resolve_spend_tx_and_check_account_balance, SpendAmount};

pub const MAX_MEMO_LENGTH: usize = 280;

pub trait WithMemo {
    fn with_memo<T: AsRef<str>>(self, memo: Option<T>) -> Self;
}

impl WithMemo for Vec<Instruction> {
    fn with_memo<T: AsRef<str>>(mut self, memo: Option<T>) -> Self {
        if let Some(memo) = &memo {
            let memo = memo.as_ref();
            let memo_ix = Instruction {
                program_id: Pubkey::new(&mundis_sdk::memo::program::id().to_bytes()),
                accounts: vec![],
                data: memo.as_bytes().to_vec(),
            };
            self.push(memo_ix);
        }
        self
    }
}
