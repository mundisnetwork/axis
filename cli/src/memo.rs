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


pub trait MemoSubCommands {
    fn memo_subcommands(self) -> Self;
}

impl MemoSubCommands for App<'_, '_> {
    fn memo_subcommands(self) -> Self {
        self.subcommand(
            SubCommand::with_name("memo")
                .about("Publish/get a memo string on Mundis")
                .setting(AppSettings::SubcommandRequiredElseHelp)
                .subcommand(
                    SubCommand::with_name("publish")
                        .about("Publish a memo on Mundis")
                        .arg(
                            Arg::with_name("memo")
                                .index(1)
                                .value_name("MEMO")
                                .takes_value(true)
                                .required(true)
                                .validator(is_short_field)
                                .help("Memo string, max. 280 chars"),
                        )
                )
        )
    }
}

// Return an error if a validator field is longer than the max length.
pub fn is_short_field(string: String) -> Result<(), String> {
    if string.len() > MAX_MEMO_LENGTH {
        Err(format!(
            "memo field longer than {:?}-byte limit",
            MAX_MEMO_LENGTH
        ))
    } else {
        Ok(())
    }
}

pub fn parse_memo_publish_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let memo = matches.value_of("memo").unwrap().to_string();
    Ok(CliCommandInfo {
        command: CliCommand::SetMemo(memo),
        signers: vec![default_signer.signer_from_path(matches, wallet_manager)?],
    })
}

pub fn parse_memo_verify_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let memo = matches.value_of("memo").unwrap().to_string();
    Ok(CliCommandInfo {
        command: CliCommand::VerifyMemo(memo),
        signers: vec![default_signer.signer_from_path(matches, wallet_manager)?],
    })
}

pub fn process_set_memo(
    rpc_client: &RpcClient,
    config: &CliConfig,
    memo: &String,
) -> ProcessResult {
    let mut instructions = Vec::new();
    instructions.push(Instruction {
        program_id: Pubkey::new(&mundis_sdk::memo::program::id().to_bytes()),
        accounts: vec![],
        data: memo.as_bytes().to_vec(),
    });
    let build_message = |lamports| Message::new(&instructions, Some(&config.signers[0].pubkey()));
    let lamports = rpc_client.get_minimum_balance_for_rent_exemption(memo.as_bytes().len() as usize)?;

    // Submit transaction
    let latest_blockhash = rpc_client.get_latest_blockhash()?;
    let (message, _) = resolve_spend_tx_and_check_account_balance(
        rpc_client,
        false,
        SpendAmount::Some(lamports),
        &latest_blockhash,
        &config.signers[0].pubkey(),
        build_message,
        config.commitment,
    )?;

    let mut tx = Transaction::new_unsigned(message);
    tx.try_sign(&vec![config.signers[0]], latest_blockhash)?;
    let signature_str = rpc_client.send_and_confirm_transaction_with_spinner(&tx)?;

    println!("Success! Memo published with signature: {}", signature_str);
    Ok("".to_string())
}
