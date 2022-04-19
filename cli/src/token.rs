use std::fmt::Display;
use std::process::exit;
use std::str::FromStr;
use std::sync::Arc;

use clap::{App, AppSettings, Arg, ArgMatches, SubCommand, value_t, value_t_or_exit};
use serde::de::Error;
use serde::Serialize;

use mundis_clap_utils::{ArgConstant, offline};
use mundis_clap_utils::fee_payer::{fee_payer_arg, FEE_PAYER_ARG};
use mundis_clap_utils::input_parsers::{pubkey_of, pubkey_of_signer, pubkeys_of_multiple_signers, signer_of, signer_of_or_else, value_of};
use mundis_clap_utils::input_validators::{is_amount, is_amount_or_all, is_parsable, is_valid_pubkey, is_valid_signer};
use mundis_clap_utils::keypair::{CliSignerInfo, CliSigners, DefaultSigner, pubkey_from_path, signer_from_path, SignerIndex};
use mundis_clap_utils::memo::{memo_arg, MEMO_ARG};
use mundis_clap_utils::nonce::{NONCE_ARG, NONCE_AUTHORITY_ARG, NonceArgs};
use mundis_clap_utils::offline::{BLOCKHASH_ARG, DUMP_TRANSACTION_MESSAGE, OfflineArgs, SIGN_ONLY_ARG};
use mundis_cli_config::Config;
use mundis_cli_output::{CliMint, CliSignature, CliSignOnlyData, OutputFormat, QuietDisplay, return_signers_data, return_signers_with_config, ReturnSignersConfig, VerboseDisplay};
use mundis_client::blockhash_query::BlockhashQuery;
use mundis_client::nonce_utils;
use mundis_client::rpc_client::RpcClient;
use mundis_memo_program::memo_instruction;
use mundis_remote_wallet::remote_wallet::RemoteWalletManager;
use mundis_sdk::instruction::Instruction;
use mundis_sdk::message::Message;
use mundis_sdk::native_token::lamports_to_mun;
use mundis_sdk::pubkey::Pubkey;
use mundis_sdk::signature::Keypair;
use mundis_sdk::signer::Signer;
use mundis_sdk::{system_instruction, system_program};
use mundis_sdk::system_instruction::SystemError;
use mundis_sdk::transaction::Transaction;
use mundis_token_account_program::get_associated_token_address;
use mundis_token_account_program::token_account_instruction::create_associated_token_account;
use mundis_token_program::native_mint;
use mundis_token_program::state::{Mint, Multisig, TokenAccount};
use mundis_token_program::token_instruction::{AuthorityType, initialize_account, initialize_mint, initialize_multisig, MAX_SIGNERS, MIN_SIGNERS};

use crate::cli::{CliCommand, CliCommandInfo, CliConfig, CliError, create_tx_info, log_instruction_custom_error, ProcessResult, TxInfo};
use crate::memo::WithMemo;
use crate::nonce::check_nonce_account;

pub const OWNER_ADDRESS_ARG: ArgConstant<'static> = ArgConstant {
    name: "owner",
    long: "owner",
    help: "Address of the token's owner. Defaults to the client keypair address.",
};

pub const OWNER_KEYPAIR_ARG: ArgConstant<'static> = ArgConstant {
    name: "owner",
    long: "owner",
    help: "Keypair of the token's owner. Defaults to the client keypair.",
};

pub const MINT_ADDRESS_ARG: ArgConstant<'static> = ArgConstant {
    name: "mint_address",
    long: "mint-address",
    help: "Address of mint that token account is associated with. Required by --sign-only",
};

pub const MINT_DECIMALS_ARG: ArgConstant<'static> = ArgConstant {
    name: "mint_decimals",
    long: "mint-decimals",
    help: "Decimals of mint that token account is associated with. Required by --sign-only",
};

pub const DELEGATE_ADDRESS_ARG: ArgConstant<'static> = ArgConstant {
    name: "delegate_address",
    long: "delegate-address",
    help: "Address of delegate currently assigned to token account. Required by --sign-only",
};

pub const MULTISIG_SIGNER_ARG: ArgConstant<'static> = ArgConstant {
    name: "multisig_signer",
    long: "multisig-signer",
    help: "Member signer of a multisig account",
};

struct SignOnlyNeedsMintDecimals {}

impl offline::ArgsConfig for SignOnlyNeedsMintDecimals {
    fn sign_only_arg<'a, 'b>(&self, arg: Arg<'a, 'b>) -> Arg<'a, 'b> {
        arg.requires_all(&[MINT_DECIMALS_ARG.name])
    }
}

struct SignOnlyNeedsFullMintSpec {}

impl offline::ArgsConfig for SignOnlyNeedsFullMintSpec {
    fn sign_only_arg<'a, 'b>(&self, arg: Arg<'a, 'b>) -> Arg<'a, 'b> {
        arg.requires_all(&[MINT_ADDRESS_ARG.name, MINT_DECIMALS_ARG.name])
    }
}

struct SignOnlyNeedsMintAddress {}

impl offline::ArgsConfig for SignOnlyNeedsMintAddress {
    fn sign_only_arg<'a, 'b>(&self, arg: Arg<'a, 'b>) -> Arg<'a, 'b> {
        arg.requires_all(&[MINT_ADDRESS_ARG.name])
    }
}

struct SignOnlyNeedsDelegateAddress {}

impl offline::ArgsConfig for SignOnlyNeedsDelegateAddress {
    fn sign_only_arg<'a, 'b>(&self, arg: Arg<'a, 'b>) -> Arg<'a, 'b> {
        arg.requires_all(&[DELEGATE_ADDRESS_ARG.name])
    }
}

pub trait MintArgs {
    fn mint_args(self) -> Self;
}

impl MintArgs for App<'_, '_> {
    fn mint_args(self) -> Self {
        self.arg(mint_address_arg().requires(MINT_DECIMALS_ARG.name))
            .arg(mint_decimals_arg().requires(MINT_ADDRESS_ARG.name))
    }
}

enum TransactionReturnData {
    CliSignature(CliSignature),
    CliSignOnlyData(CliSignOnlyData),
}

pub trait TokenSubCommands {
    fn token_subcommands(self) -> Self;
}

impl TokenSubCommands for App<'_, '_> {
    fn token_subcommands<'a, 'b>(self) -> Self {
        self.subcommand(
            SubCommand::with_name("token")
                .about("Manage Mundis custom tokens")
                .setting(AppSettings::SubcommandRequiredElseHelp)
                .arg(
                    Arg::with_name("use_unchecked_instruction")
                        .long("use-unchecked-instruction")
                        .takes_value(false)
                        .global(true)
                        .hidden(true)
                        .help("Use unchecked instruction if appropriate. Supports transfer, burn, mint, and approve."),
                )
                .arg(fee_payer_arg().global(true))
                .subcommand(
                    SubCommand::with_name("create-token")
                        .about("Create a new token")
                        .nonce_args(true)
                        .arg(memo_arg())
                        .offline_args()
                        .arg(
                            Arg::with_name("token_keypair")
                                .value_name("TOKEN_KEYPAIR")
                                .validator(is_valid_signer)
                                .takes_value(true)
                                .index(1)
                                .help(
                                    "Specify the token keypair. \
                             This may be a keypair file or the ASK keyword. \
                             [default: randomly generated keypair]"
                                ),
                        )
                        .arg(
                            Arg::with_name("mint_authority")
                                .long("mint-authority")
                                .alias("owner")
                                .value_name("ADDRESS")
                                .validator(is_valid_pubkey)
                                .takes_value(true)
                                .help(
                                    "Specify the mint authority address. \
                             Defaults to the client keypair address."
                                ),
                        )
                        .arg(
                            Arg::with_name("decimals")
                                .long("decimals")
                                .validator(is_mint_decimals)
                                .value_name("DECIMALS")
                                .takes_value(true)
                                .default_value(&formatcp!("{}", native_mint::DECIMALS))
                                .help("Number of base 10 digits to the right of the decimal place"),
                        )
                        .arg(
                            Arg::with_name("enable_freeze")
                                .long("enable-freeze")
                                .takes_value(false)
                                .help(
                                    "Enable the mint authority to freeze associated token accounts."
                                ),
                        )
                )
                .subcommand(
                    SubCommand::with_name("create-account")
                        .about("Create a new token account")
                        .arg(owner_address_arg())
                        .nonce_args(true)
                        .offline_args()
                        .arg(
                            Arg::with_name("token")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("The token that the account will hold"),
                        )
                        .arg(
                            Arg::with_name("account_keypair")
                                .value_name("ACCOUNT_KEYPAIR")
                                .validator(is_valid_signer)
                                .takes_value(true)
                                .index(2)
                                .help(
                                    "Specify the account keypair. \
                             This may be a keypair file or the ASK keyword. \
                             [default: associated token account for --owner]"
                                ),
                        )
                )
                .subcommand(
                    SubCommand::with_name("create-multisig")
                        .about("Create a new account describing an M:N multisignature")
                        .arg(
                            Arg::with_name("minimum_signers")
                                .value_name("MINIMUM_SIGNERS")
                                .validator(is_multisig_minimum_signers)
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help(&formatcp!("The minimum number of signers required \
                            to allow the operation. [{} <= M <= N]",
                                               MIN_SIGNERS,
                                )),
                        )
                        .arg(
                            Arg::with_name("multisig_member")
                                .value_name("MULTISIG_MEMBER_PUBKEY")
                                .validator(is_valid_pubkey)
                                .takes_value(true)
                                .index(2)
                                .required(true)
                                .min_values(MIN_SIGNERS as u64)
                                .max_values(MAX_SIGNERS as u64)
                                .help(&formatcp!("The public keys for each of the N \
                            signing members of this account. [{} <= N <= {}]",
                                               MIN_SIGNERS, MAX_SIGNERS,
                                )),
                        )
                        .arg(
                            Arg::with_name("address_keypair")
                                .long("address-keypair")
                                .value_name("ADDRESS_KEYPAIR")
                                .validator(is_valid_signer)
                                .takes_value(true)
                                .help(
                                    "Specify the address keypair. \
                             This may be a keypair file or the ASK keyword. \
                             [default: randomly generated keypair]"
                                ),
                        )
                        .nonce_args(true)
                        .offline_args(),
                )
                .subcommand(
                    SubCommand::with_name("authorize")
                        .about("Authorize a new signing keypair to a token or token account")
                        .arg(
                            Arg::with_name("address")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("The address of the token account"),
                        )
                        .arg(
                            Arg::with_name("authority_type")
                                .value_name("AUTHORITY_TYPE")
                                .takes_value(true)
                                .possible_values(&["mint", "freeze", "owner", "close"])
                                .index(2)
                                .required(true)
                                .help("The new authority type. \
                            Token mints support `mint` and `freeze` authorities;\
                            Token accounts support `owner` and `close` authorities."),
                        )
                        .arg(
                            Arg::with_name("new_authority")
                                .validator(is_valid_pubkey)
                                .value_name("AUTHORITY_ADDRESS")
                                .takes_value(true)
                                .index(3)
                                .required_unless("disable")
                                .help("The address of the new authority"),
                        )
                        .arg(
                            Arg::with_name("authority")
                                .long("authority")
                                .alias("owner")
                                .value_name("KEYPAIR")
                                .validator(is_valid_signer)
                                .takes_value(true)
                                .help(
                                    "Specify the current authority keypair. \
                             Defaults to the client keypair."
                                ),
                        )
                        .arg(
                            Arg::with_name("disable")
                                .long("disable")
                                .takes_value(false)
                                .conflicts_with("new_authority")
                                .help("Disable mint, freeze, or close functionality by setting authority to None.")
                        )
                        .arg(
                            Arg::with_name("force")
                                .long("force")
                                .hidden(true)
                                .help("Force re-authorize the wallet's associate token account. Don't use this flag"),
                        )
                        .arg(multisig_signer_arg())
                        .nonce_args(true)
                        .offline_args(),
                )
                .subcommand(
                    SubCommand::with_name("transfer")
                        .about("Transfer tokens between accounts")
                        .arg(
                            Arg::with_name("token")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("Token to transfer"),
                        )
                        .arg(
                            Arg::with_name("amount")
                                .validator(is_amount_or_all)
                                .value_name("TOKEN_AMOUNT")
                                .takes_value(true)
                                .index(2)
                                .required(true)
                                .help("Amount to send, in tokens; accepts keyword ALL"),
                        )
                        .arg(
                            Arg::with_name("recipient")
                                .validator(is_valid_pubkey)
                                .value_name("RECIPIENT_ADDRESS or RECIPIENT_TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .index(3)
                                .required(true)
                                .help("If a token account address is provided, use it as the recipient. \
                               Otherwise assume the recipient address is a user wallet and transfer to \
                               the associated token account")
                        )
                        .arg(
                            Arg::with_name("from")
                                .validator(is_valid_pubkey)
                                .value_name("SENDER_TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .long("from")
                                .help("Specify the sending token account \
                            [default: owner's associated token account]")
                        )
                        .arg(owner_keypair_arg_with_value_name("SENDER_TOKEN_OWNER_KEYPAIR")
                                 .help(
                                     "Specify the owner of the sending token account. \
                            This may be a keypair file, the ASK keyword. \
                            Defaults to the client keypair.",
                                 ),
                        )
                        .arg(
                            Arg::with_name("allow_unfunded_recipient")
                                .long("allow-unfunded-recipient")
                                .takes_value(false)
                                .help("Complete the transfer even if the recipient address is not funded")
                        )
                        .arg(
                            Arg::with_name("allow_empty_recipient")
                                .long("allow-empty-recipient")
                                .takes_value(false)
                                .hidden(true) // Deprecated, use --allow-unfunded-recipient instead
                        )
                        .arg(
                            Arg::with_name("fund_recipient")
                                .long("fund-recipient")
                                .takes_value(false)
                                .help("Create the associated token account for the recipient if doesn't already exist")
                        )
                        .arg(
                            Arg::with_name("no_wait")
                                .long("no-wait")
                                .takes_value(false)
                                .help("Return signature immediately after submitting the transaction, instead of waiting for confirmations"),
                        )
                        .arg(
                            Arg::with_name("recipient_is_ata_owner")
                                .long("recipient-is-ata-owner")
                                .takes_value(false)
                                .requires("sign_only")
                                .help("In sign-only mode, specifies that the recipient is the owner of the associated token account rather than an actual token account"),
                        )
                        .arg(multisig_signer_arg())
                        .arg(mint_decimals_arg())
                        .nonce_args(true)
                        .arg(memo_arg())
                        .offline_args_config(&SignOnlyNeedsMintDecimals {}),
                )
                .subcommand(
                    SubCommand::with_name("burn")
                        .about("Burn tokens from an account")
                        .arg(
                            Arg::with_name("source")
                                .validator(is_valid_pubkey)
                                .value_name("SOURCE_TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("The token account address to burn from"),
                        )
                        .arg(
                            Arg::with_name("amount")
                                .validator(is_amount)
                                .value_name("TOKEN_AMOUNT")
                                .takes_value(true)
                                .index(2)
                                .required(true)
                                .help("Amount to burn, in tokens"),
                        )
                        .arg(owner_keypair_arg_with_value_name("SOURCE_TOKEN_OWNER_KEYPAIR")
                                 .help(
                                     "Specify the source token owner account. \
                            This may be a keypair file, the ASK keyword. \
                            Defaults to the client keypair.",
                                 ),
                        )
                        .arg(multisig_signer_arg())
                        .mint_args()
                        .nonce_args(true)
                        .arg(memo_arg())
                        .offline_args_config(&SignOnlyNeedsFullMintSpec {}),
                )
                .subcommand(
                    SubCommand::with_name("mint")
                        .about("Mint new tokens")
                        .arg(
                            Arg::with_name("token")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("The token to mint"),
                        )
                        .arg(
                            Arg::with_name("amount")
                                .validator(is_amount)
                                .value_name("TOKEN_AMOUNT")
                                .takes_value(true)
                                .index(2)
                                .required(true)
                                .help("Amount to mint, in tokens"),
                        )
                        .arg(
                            Arg::with_name("recipient")
                                .validator(is_valid_pubkey)
                                .value_name("RECIPIENT_TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .index(3)
                                .help("The token account address of recipient [default: associated token account for --owner]"),
                        )
                        .arg(
                            Arg::with_name("mint_authority")
                                .long("mint-authority")
                                .alias("owner")
                                .value_name("KEYPAIR")
                                .validator(is_valid_signer)
                                .takes_value(true)
                                .help(
                                    "Specify the mint authority keypair. \
                             This may be a keypair file or the ASK keyword. \
                             Defaults to the client keypair."
                                ),
                        )
                        .arg(mint_decimals_arg())
                        .arg(multisig_signer_arg())
                        .nonce_args(true)
                        .offline_args_config(&SignOnlyNeedsMintDecimals {}),
                )
                .subcommand(
                    SubCommand::with_name("freeze")
                        .about("Freeze a token account")
                        .arg(
                            Arg::with_name("account")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("The address of the token account to freeze"),
                        )
                        .arg(
                            Arg::with_name("freeze_authority")
                                .long("freeze-authority")
                                .alias("owner")
                                .value_name("KEYPAIR")
                                .validator(is_valid_signer)
                                .takes_value(true)
                                .help(
                                    "Specify the freeze authority keypair. \
                                 This may be a keypair file or the ASK keyword. \
                                 Defaults to the client keypair."
                                ),
                        )
                        .arg(mint_address_arg())
                        .arg(multisig_signer_arg())
                        .nonce_args(true)
                        .offline_args_config(&SignOnlyNeedsMintAddress {}),
                )
                .subcommand(
                    SubCommand::with_name("thaw")
                        .about("Thaw a token account")
                        .arg(
                            Arg::with_name("account")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("The address of the token account to thaw"),
                        )
                        .arg(
                            Arg::with_name("freeze_authority")
                                .long("freeze-authority")
                                .alias("owner")
                                .value_name("KEYPAIR")
                                .validator(is_valid_signer)
                                .takes_value(true)
                                .help(
                                    "Specify the freeze authority keypair. \
                             This may be a keypair file or the ASK keyword. \
                             Defaults to the client keypair."
                                ),
                        )
                        .arg(mint_address_arg())
                        .arg(multisig_signer_arg())
                        .nonce_args(true)
                        .offline_args_config(&SignOnlyNeedsMintAddress {}),
                )
                .subcommand(
                    SubCommand::with_name("wrap")
                        .about("Wrap native MUN in a MUN token account")
                        .arg(
                            Arg::with_name("amount")
                                .validator(is_amount)
                                .value_name("AMOUNT")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("Amount of SOL to wrap"),
                        )
                        .arg(
                            Arg::with_name("wallet_keypair")
                                .alias("owner")
                                .value_name("KEYPAIR")
                                .validator(is_valid_signer)
                                .takes_value(true)
                                .help(
                                    "Specify the keypair for the wallet which will have its native SOL wrapped. \
                             This wallet will be assigned as the owner of the wrapped SOL token account. \
                             This may be a keypair file or the ASK keyword. \
                             Defaults to the client keypair."
                                ),
                        )
                        .arg(
                            Arg::with_name("create_aux_account")
                                .takes_value(false)
                                .long("create-aux-account")
                                .help("Wrap SOL in an auxiliary account instead of associated token account"),
                        )
                        .nonce_args(true)
                        .offline_args(),
                )
                .subcommand(
                    SubCommand::with_name("unwrap")
                        .about("Unwrap a SOL token account")
                        .arg(
                            Arg::with_name("address")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .help("The address of the auxiliary token account to unwrap \
                            [default: associated token account for --owner]"),
                        )
                        .arg(
                            Arg::with_name("wallet_keypair")
                                .alias("owner")
                                .value_name("KEYPAIR")
                                .validator(is_valid_signer)
                                .takes_value(true)
                                .help(
                                    "Specify the keypair for the wallet which owns the wrapped SOL. \
                             This wallet will receive the unwrapped SOL. \
                             This may be a keypair file or the ASK keyword. \
                             Defaults to the client keypair."
                                ),
                        )
                        .arg(multisig_signer_arg())
                        .nonce_args(true)
                        .offline_args(),
                )
                .subcommand(
                    SubCommand::with_name("approve")
                        .about("Approve a delegate for a token account")
                        .arg(
                            Arg::with_name("account")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("The address of the token account to delegate"),
                        )
                        .arg(
                            Arg::with_name("amount")
                                .validator(is_amount)
                                .value_name("TOKEN_AMOUNT")
                                .takes_value(true)
                                .index(2)
                                .required(true)
                                .help("Amount to approve, in tokens"),
                        )
                        .arg(
                            Arg::with_name("delegate")
                                .validator(is_valid_pubkey)
                                .value_name("DELEGATE_TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .index(3)
                                .required(true)
                                .help("The token account address of delegate"),
                        )
                        .arg(
                            owner_keypair_arg()
                        )
                        .arg(multisig_signer_arg())
                        .mint_args()
                        .nonce_args(true)
                        .offline_args_config(&SignOnlyNeedsFullMintSpec {}),
                )
                .subcommand(
                    SubCommand::with_name("revoke")
                        .about("Revoke a delegate's authority")
                        .arg(
                            Arg::with_name("account")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("The address of the token account"),
                        )
                        .arg(owner_keypair_arg()
                        )
                        .arg(delegate_address_arg())
                        .arg(multisig_signer_arg())
                        .nonce_args(true)
                        .offline_args_config(&SignOnlyNeedsDelegateAddress {}),
                )
                .subcommand(
                    SubCommand::with_name("close")
                        .about("Close a token account")
                        .arg(
                            Arg::with_name("token")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required_unless("address")
                                .help("Token to close. To close a specific account, use the `--address` parameter instead"),
                        )
                        .arg(owner_address_arg())
                        .arg(
                            Arg::with_name("recipient")
                                .long("recipient")
                                .validator(is_valid_pubkey)
                                .value_name("REFUND_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .help("The address of the account to receive remaining SOL [default: --owner]"),
                        )
                        .arg(
                            Arg::with_name("close_authority")
                                .long("close-authority")
                                .alias("owner")
                                .value_name("KEYPAIR")
                                .validator(is_valid_signer)
                                .takes_value(true)
                                .help(
                                    "Specify the token's close authority if it has one, \
                            otherwise specify the token's owner keypair. \
                            This may be a keypair file, the ASK keyword. \
                            Defaults to the client keypair.",
                                ),
                        )
                        .arg(
                            Arg::with_name("address")
                                .long("address")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .conflicts_with("token")
                                .help("Specify the token account to close \
                            [default: owner's associated token account]"),
                        )
                        .arg(multisig_signer_arg())
                        .nonce_args(true)
                        .offline_args(),
                )
                .subcommand(
                    SubCommand::with_name("balance")
                        .about("Get token account balance")
                        .arg(
                            Arg::with_name("token")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required_unless("address")
                                .help("Token of associated account. To query a specific account, use the `--address` parameter instead"),
                        )
                        .arg(owner_address_arg().conflicts_with("address"))
                        .arg(
                            Arg::with_name("address")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .long("address")
                                .conflicts_with("token")
                                .help("Specify the token account to query \
                            [default: owner's associated token account]"),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("supply")
                        .about("Get token supply")
                        .arg(
                            Arg::with_name("address")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("The token address"),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("accounts")
                        .about("List all token accounts by owner")
                        .arg(
                            Arg::with_name("token")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .help("Limit results to the given token. [Default: list accounts for all tokens]"),
                        )
                        .arg(owner_address_arg())
                )
                .subcommand(
                    SubCommand::with_name("address")
                        .about("Get wallet address")
                        .arg(
                            Arg::with_name("token")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ADDRESS")
                                .takes_value(true)
                                .long("token")
                                .requires("verbose")
                                .help("Return the associated token address for the given token. \
                               [Default: return the client keypair address]")
                        )
                        .arg(
                            owner_address_arg()
                                .requires("token")
                                .help("Return the associated token address for the given owner. \
                               [Default: return the associated token address for the client keypair]"),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("account-info")
                        .about("Query details of aa token account by address")
                        .arg(
                            Arg::with_name("token")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .conflicts_with("address")
                                .required_unless("address")
                                .help("Token of associated account. \
                               To query a specific account, use the `--address` parameter instead"),
                        )
                        .arg(
                            owner_address_arg()
                                .index(2)
                                .conflicts_with("address")
                                .help("Owner of the associated account for the specified token. \
                               To query a specific account, use the `--address` parameter instead. \
                               Defaults to the client keypair."),
                        )
                        .arg(
                            Arg::with_name("address")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .long("address")
                                .conflicts_with("token")
                                .help("Specify the token account to query"),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("multisig-info")
                        .about("Query details about token multisig account by address")
                        .arg(
                            Arg::with_name("address")
                                .validator(is_valid_pubkey)
                                .value_name("MULTISIG_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("The address of the SPL Token multisig account to query"),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("cleanup")
                        .about("Cleanup unnecessary token accounts")
                        .arg(owner_keypair_arg())
                        .arg(
                            Arg::with_name("close_empty_associated_accounts")
                                .long("close-empty-associated-accounts")
                                .takes_value(false)
                                .help("close all empty associated token accounts (to get SOL back)")
                        )
                )
                .subcommand(
                    SubCommand::with_name("sync-native")
                        .about("Sync a native MUN token account to its underlying lamports")
                        .arg(
                            owner_address_arg()
                                .index(1)
                                .conflicts_with("address")
                                .help("Owner of the associated account for the native token. \
                               To query a specific account, use the `--address` parameter instead. \
                               Defaults to the client keypair."),
                        )
                        .arg(
                            Arg::with_name("address")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .long("address")
                                .conflicts_with("owner")
                                .help("Specify the specific token account address to sync"),
                        ),
                )
        )
    }
}

pub fn owner_address_arg<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name(OWNER_ADDRESS_ARG.name)
        .long(OWNER_ADDRESS_ARG.long)
        .takes_value(true)
        .value_name("OWNER_ADDRESS")
        .validator(is_valid_pubkey)
        .help(OWNER_ADDRESS_ARG.help)
}

pub fn mint_decimals_arg<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name(MINT_DECIMALS_ARG.name)
        .long(MINT_DECIMALS_ARG.long)
        .takes_value(true)
        .value_name("MINT_DECIMALS")
        .validator(is_mint_decimals)
        .requires(SIGN_ONLY_ARG.name)
        .requires(BLOCKHASH_ARG.name)
        .help(MINT_DECIMALS_ARG.help)
}

pub fn multisig_signer_arg<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name(MULTISIG_SIGNER_ARG.name)
        .long(MULTISIG_SIGNER_ARG.long)
        .validator(is_valid_signer)
        .value_name("MULTISIG_SIGNER")
        .takes_value(true)
        .multiple(true)
        .min_values(0u64)
        .max_values(MAX_SIGNERS as u64)
        .help(MULTISIG_SIGNER_ARG.help)
}

pub fn owner_keypair_arg<'a, 'b>() -> Arg<'a, 'b> {
    owner_keypair_arg_with_value_name("OWNER_KEYPAIR")
}

pub fn owner_keypair_arg_with_value_name<'a, 'b>(value_name: &'static str) -> Arg<'a, 'b> {
    Arg::with_name(OWNER_KEYPAIR_ARG.name)
        .long(OWNER_KEYPAIR_ARG.long)
        .takes_value(true)
        .value_name(value_name)
        .validator(is_valid_signer)
        .help(OWNER_KEYPAIR_ARG.help)
}

pub fn mint_address_arg<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name(MINT_ADDRESS_ARG.name)
        .long(MINT_ADDRESS_ARG.long)
        .takes_value(true)
        .value_name("MINT_ADDRESS")
        .validator(is_valid_pubkey)
        .requires(SIGN_ONLY_ARG.name)
        .requires(BLOCKHASH_ARG.name)
        .help(MINT_ADDRESS_ARG.help)
}

pub fn delegate_address_arg<'a, 'b>() -> Arg<'a, 'b> {
    Arg::with_name(DELEGATE_ADDRESS_ARG.name)
        .long(DELEGATE_ADDRESS_ARG.long)
        .takes_value(true)
        .value_name("DELEGATE_ADDRESS")
        .validator(is_valid_pubkey)
        .requires(SIGN_ONLY_ARG.name)
        .requires(BLOCKHASH_ARG.name)
        .help(DELEGATE_ADDRESS_ARG.help)
}

fn is_mint_decimals(string: String) -> Result<(), String> {
    is_parsable::<u8>(string)
}

fn is_multisig_minimum_signers(string: String) -> Result<(), String> {
    let v = u8::from_str(&string).map_err(|e| e.to_string())? as usize;
    if v < MIN_SIGNERS {
        Err(format!("must be at least {}", MIN_SIGNERS))
    } else if v > MAX_SIGNERS {
        Err(format!("must be at most {}", MAX_SIGNERS))
    } else {
        Ok(())
    }
}

pub fn add_default_signers(
    matches: &ArgMatches<'_>,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
    bulk_signers: &mut Vec<Option<Box<dyn Signer>>>,
) -> Result<(Option<Pubkey>, Option<Pubkey>, Option<Pubkey>), CliError> {
    // fee payer
    let (fee_payer, fee_payer_pubkey) = signer_of(matches, FEE_PAYER_ARG.name, wallet_manager)?;
    bulk_signers.push(fee_payer);

    // nonce account
    let nonce_account = pubkey_of(matches, NONCE_ARG.name);
    let (nonce_authority, nonce_authority_pubkey) =
        signer_of(matches, NONCE_AUTHORITY_ARG.name, wallet_manager)?;
    if nonce_account.is_some() {
        bulk_signers.push(nonce_authority);
    }

    // multisig signers
    let multisig_signers = signers_of(matches, MULTISIG_SIGNER_ARG.name, wallet_manager)
        .unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            exit(1);
        });

    if let Some(mut multisig_signers) = multisig_signers {
        multisig_signers.sort_by(|(_, lp), (_, rp)| lp.cmp(rp));
        let (signers, _): (Vec<_>, Vec<_>) = multisig_signers.into_iter().unzip();
        bulk_signers.extend(signers);
    }

   Ok((fee_payer_pubkey, nonce_account, nonce_authority_pubkey))
}

pub fn parse_create_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let decimals = value_t_or_exit!(matches, "decimals", u8);
    let mint_authority = pubkey_or_default(matches, "mint_authority", default_signer, wallet_manager);
    let (token_signer, token) = signer_of_or_else(matches, "token_keypair", wallet_manager, new_throwaway_signer)?;

    bulk_signers.push(Some(token_signer.unwrap()));

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::CreateToken {
            token: token.unwrap(),
            authority: mint_authority,
            decimals,
            enable_freeze: matches.is_present("enable_freeze"),
            memo: matches.value_of(MEMO_ARG.name).map(String::from),
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_create_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    token: &Pubkey,
    authority: Pubkey,
    decimals: u8,
    enable_freeze: bool,
    memo: Option<&String>,
    tx_info: &TxInfo
) -> ProcessResult {
    println_display(config, format!("Creating token {}", token));

    let minimum_balance_for_rent_exemption = if !tx_info.sign_only {
        rpc_client.get_minimum_balance_for_rent_exemption(Mint::packed_len())?
    } else {
        0
    };

    let freeze_authority_pubkey = if enable_freeze { Some(authority) } else { None };

    let instructions = vec![
        system_instruction::create_account(
            &config.signers[tx_info.fee_payer].pubkey(),
            &token,
            1,
            Mint::packed_len() as u64,
            &mundis_token_program::id(),
        ),
        initialize_mint(
            &mundis_token_program::id(),
            &token,
            &authority,
            freeze_authority_pubkey.as_ref(),
            decimals,
        )?,
    ].with_memo(memo);

    let tx_return = handle_tx(
        rpc_client,
        config,
        minimum_balance_for_rent_exemption,
        instructions,
        tx_info
    )?;

    Ok(match tx_return {
        TransactionReturnData::CliSignature(cli_signature) => config.output_format.formatted_string(
            &CliMint {
                address: token.to_string(),
                decimals,
                transaction_data: cli_signature,
            },
        ),
        TransactionReturnData::CliSignOnlyData(ref cli_sign_only_data) => {
            config.output_format.formatted_string(cli_sign_only_data)
        }
    })
}

pub fn parse_create_token_account_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let token = pubkey_of_signer(matches, "token", wallet_manager)
        .unwrap()
        .unwrap();

    // No need to add a signer when creating an associated token account
    let (account, account_key) = signer_of(matches, "account_keypair", wallet_manager)?;
    if account.is_some() {
        bulk_signers.push(account);
    }

    let owner = pubkey_or_default(matches, "owner", default_signer, wallet_manager);

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::CreateTokenAccount {
            token,
            owner,
            account: account_key,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_create_token_account_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    token: Pubkey,
    owner: Pubkey,
    maybe_account: Option<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    let fee_payer = config.signers[tx_info.fee_payer];

    let minimum_balance_for_rent_exemption = if !tx_info.sign_only {
        rpc_client.get_minimum_balance_for_rent_exemption(TokenAccount::packed_len())?
    } else {
        0
    };

    let (account, system_account_ok, instructions) = if let Some(account) = maybe_account {
        println_display(config, format!("Creating account {}", account));
        (
            account,
            false,
            vec![
                system_instruction::create_account(
                    &fee_payer.pubkey(),
                    &account,
                    minimum_balance_for_rent_exemption,
                    TokenAccount::packed_len() as u64,
                    &mundis_token_program::id(),
                ),
                initialize_account(&mundis_token_program::id(), &account, &token, &owner)?,
            ],
        )
    } else {
        let account = get_associated_token_address(&owner, &token);
        println_display(config, format!("Creating account {}", account));
        (
            account,
            true,
            vec![create_associated_token_account(
                &fee_payer.pubkey(),
                &owner,
                &token,
            )],
        )
    };

    if !tx_info.sign_only {
        if let Some(account_data) = rpc_client.get_account_with_commitment(&account, config.commitment)?
            .value
        {
            if !(account_data.owner == system_program::id() && system_account_ok) {
                return Err(format!("Error: Account already exists: {}", account).into());
            }
        }
    }

    let tx_return = handle_tx(
        rpc_client,
        config,
        minimum_balance_for_rent_exemption,
        instructions,
        tx_info
    )?;

    Ok(match tx_return {
        TransactionReturnData::CliSignature(signature) => {
            config.output_format.formatted_string(&signature)
        }
        TransactionReturnData::CliSignOnlyData(sign_only_data) => {
            config.output_format.formatted_string(&sign_only_data)
        }
    })
}

pub fn parse_create_multisig_token_account_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let minimum_signers = value_of::<u8>(matches, "minimum_signers").unwrap();
    let multisig_members =
        pubkeys_of_multiple_signers(matches, "multisig_member", wallet_manager)
            .unwrap_or_else(|e| {
                eprintln!("error: {}", e);
                exit(1);
            })
            .unwrap();

    if minimum_signers as usize > multisig_members.len() {
        eprintln!(
            "error: MINIMUM_SIGNERS cannot be greater than the number \
                          of MULTISIG_MEMBERs passed"
        );
        exit(1);
    }

    let (signer, account) = signer_of_or_else(matches, "address_keypair", wallet_manager, new_throwaway_signer)?;

    bulk_signers.push(signer);

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::CreateMultisigToken {
            multisig: account.unwrap(),
            minimum_signers,
            multisig_members,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_create_multisig_token_account_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    multisig: Pubkey,
    minimum_signers: u8,
    multisig_members: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    println_display(
        config,
        format!(
            "Creating {}/{} multisig {}",
            minimum_signers,
            multisig_members.len(),
            multisig
        ),
    );

    let minimum_balance_for_rent_exemption = if !tx_info.sign_only {
        rpc_client.get_minimum_balance_for_rent_exemption(Multisig::packed_len())?
    } else {
        0
    };

    let fee_payer = config.signers[tx_info.fee_payer];
    let instructions = vec![
        system_instruction::create_account(
            &fee_payer.pubkey(),
            &multisig,
            minimum_balance_for_rent_exemption,
            Multisig::packed_len() as u64,
            &mundis_token_program::id(),
        ),
        initialize_multisig(
            &mundis_token_program::id(),
            &multisig,
            multisig_members.iter().collect::<Vec<_>>().as_slice(),
            minimum_signers,
        )?,
    ];

    let tx_return = handle_tx(
        rpc_client,
        config,
        minimum_balance_for_rent_exemption,
        instructions,
        tx_info
    )?;

    Ok(match tx_return {
        TransactionReturnData::CliSignature(signature) => {
            config.output_format.formatted_string(&signature)
        }
        TransactionReturnData::CliSignOnlyData(sign_only_data) => {
            config.output_format.formatted_string(&sign_only_data)
        }
    })
}

pub fn parse_authorize_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let address = pubkey_of_signer(matches, "address", wallet_manager)
        .unwrap()
        .unwrap();

    let authority_type = matches.value_of("authority_type").unwrap();
    let authority_type = match authority_type {
        "mint" => AuthorityType::MintTokens,
        "freeze" => AuthorityType::FreezeAccount,
        "owner" => AuthorityType::AccountOwner,
        "close" => AuthorityType::CloseAccount,
        _ => unreachable!(),
    };

    let (authority_signer, authority) = signer_of(matches, "authority",  wallet_manager)?;
    if authority.is_some() {
        bulk_signers.push(authority_signer);
    } else {
        bulk_signers.push(Some(default_signer.signer_from_path(matches, wallet_manager)?));
    }

    let new_authority = pubkey_of_signer(matches, "new_authority", wallet_manager)?;
    let force_authorize = matches.is_present("force");

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::AuthorizeToken {
            account: address,
            authority_type,
            authority,
            new_authority,
            force_authorize,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_authorize_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    account: Pubkey,
    option: Option<Pubkey>,
    authority_type: &AuthorityType,
    new_authority: Option<Pubkey>,
    force_authorize: bool,
    tx_info: &TxInfo,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_transfer_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::TransferToken,
        signers: CliSigners::new(),
    })
}

pub fn process_transfer_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_burn_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::BurnToken,
        signers: CliSigners::new(),
    })
}

pub fn process_burn_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_mint_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::MintToken,
        signers: CliSigners::new(),
    })
}

pub fn process_mint_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_freeze_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::FreezeTokenAccount,
        signers: CliSigners::new(),
    })
}

pub fn process_freeze_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_thaw_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::ThawTokenAccount,
        signers: CliSigners::new(),
    })
}

pub fn process_thaw_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_wrap_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::WrapToken,
        signers: CliSigners::new(),
    })
}

pub fn process_wrap_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_unwrap_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::UnwrapToken,
        signers: CliSigners::new(),
    })
}

pub fn process_unwrap_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_approve_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::ApproveToken,
        signers: CliSigners::new(),
    })
}

pub fn process_approve_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_revoke_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::RevokeToken,
        signers: CliSigners::new(),
    })
}

pub fn process_revoke_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_close_token_account_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::CloseTokenAccount,
        signers: CliSigners::new(),
    })
}

pub fn process_close_token_account_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_token_balance_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::GetTokenAccountBalance,
        signers: CliSigners::new(),
    })
}

pub fn process_token_balance_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_token_supply_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::GetTokenSupply,
        signers: CliSigners::new(),
    })
}

pub fn process_token_supply_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_token_list_accounts_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::ListTokenAccounts,
        signers: CliSigners::new(),
    })
}

pub fn process_token_list_accounts_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_token_wallet_address_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::GetTokenWalletAddress,
        signers: CliSigners::new(),
    })
}

pub fn process_token_wallet_address_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_token_account_info_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::GetTokenAccountByAddress,
        signers: CliSigners::new(),
    })
}

pub fn process_token_account_info_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_token_multisig_info_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::GetTokenMultisigAccountByAddress,
        signers: CliSigners::new(),
    })
}

pub fn process_token_multisig_info_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_token_cleanup_accounts_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::CleanupTokenAccounts,
        signers: CliSigners::new(),
    })
}

pub fn process_token_cleanup_accounts_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_token_sync_native_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::SyncTokenAccount,
        signers: CliSigners::new(),
    })
}

pub fn process_token_sync_native_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

// Checks if an explicit address was provided, otherwise return the default address.
fn pubkey_or_default(
    arg_matches: &ArgMatches,
    address_name: &str,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Pubkey {
    if address_name != "owner" {
        if let Some(address) =
        pubkey_of_signer(arg_matches, address_name, wallet_manager).unwrap() {
            return address;
        }
    }

    return default_address(arg_matches, default_signer, wallet_manager)
        .unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            exit(1);
        });
}

fn default_address(
    matches: &ArgMatches,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<Pubkey, Box<dyn std::error::Error>> {
    if let Some(address) = pubkey_of_signer(matches, "owner", wallet_manager).unwrap() {
        return Ok(address);
    }

    pubkey_from_path(matches, &default_signer.path, "default", wallet_manager)
}

type SignersOf = Vec<(Option<Box<dyn Signer>>, Option<Pubkey>)>;
pub fn signers_of(
    matches: &ArgMatches<'_>,
    name: &str,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<Option<SignersOf>, Box<dyn std::error::Error>> {
    if let Some(values) = matches.values_of(name) {
        let mut results = Vec::new();
        for (i, value) in values.enumerate() {
            let name = format!("{}-{}", name, i + 1);
            let signer = signer_from_path(matches, value, &name, wallet_manager)?;
            let signer_pubkey = signer.pubkey();
            results.push((Some(signer), Some(signer_pubkey)));
        }
        Ok(Some(results))
    } else {
        Ok(None)
    }
}

fn check_fee_payer_balance(rpc_client: &RpcClient, fee_payer: &Pubkey, required_balance: u64) -> Result<(), Box<dyn std::error::Error>> {
    let balance = rpc_client.get_balance(fee_payer)?;
    if balance < required_balance {
        Err(format!(
            "Fee payer, {}, has insufficient balance: {} required, {} available",
            fee_payer,
            lamports_to_mun(required_balance),
            lamports_to_mun(balance)
        )
            .into())
    } else {
        Ok(())
    }
}

fn handle_tx(
    rpc_client: &RpcClient,
    config: &CliConfig,
    minimum_balance_for_rent_exemption: u64,
    instructions: Vec<Instruction>,
    tx_info: &TxInfo
) -> Result<TransactionReturnData, Box<dyn std::error::Error>> {
    let recent_blockhash = tx_info.blockhash_query.get_blockhash(rpc_client, config.commitment)?;

    let mut message = if let Some(nonce_account) = &tx_info.nonce_account {
        Message::new_with_nonce(
            instructions,
            Some(&config.signers[tx_info.fee_payer].pubkey()),
            nonce_account,
            &config.signers[tx_info.nonce_authority].pubkey(),
        )
    } else {
        Message::new(&instructions, Some(&config.signers[tx_info.fee_payer].pubkey()))
    };

    message.recent_blockhash = recent_blockhash;
    let message_fee = rpc_client.get_fee_for_message(&message)
        .unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            exit(1);
        });

    if !tx_info.sign_only {
        check_fee_payer_balance(
            rpc_client,
            &config.signers[tx_info.fee_payer].pubkey(),
            minimum_balance_for_rent_exemption + message_fee,
        )?;
    }

    let mut tx = Transaction::new_unsigned(message);
    if tx_info.sign_only {
        tx.try_partial_sign(&config.signers, recent_blockhash)?;
        Ok(TransactionReturnData::CliSignOnlyData(return_signers_data(
            &tx,
            &ReturnSignersConfig {
                dump_transaction_message: tx_info.dump_transaction_message,
            }
        )))
    } else {
        tx.try_sign(&config.signers, recent_blockhash)?;
        let signature = if tx_info.no_wait {
            rpc_client.send_transaction(&tx)?
        } else {
            rpc_client.send_and_confirm_transaction_with_spinner(&tx)?
        };
        Ok(TransactionReturnData::CliSignature(CliSignature {
            signature: signature.to_string(),
        }))
    }
}

pub(crate) fn println_display(config: &CliConfig, message: String) {
    match config.output_format {
        OutputFormat::Display | OutputFormat::DisplayVerbose => {
            println!("{}", message);
        }
        _ => {}
    }
}

fn new_throwaway_signer() -> (Option<Box<dyn Signer>>, Option<Pubkey>) {
    let keypair = Keypair::new();
    let pubkey = keypair.pubkey();
    (Some(Box::new(keypair) as Box<dyn Signer>), Some(pubkey))
}