use std::process::exit;
use std::str::FromStr;
use std::sync::Arc;
use clap::{App, AppSettings, Arg, ArgMatches, SubCommand, value_t, value_t_or_exit};
use serde::de::Error;
use mundis_clap_utils::{ArgConstant, offline};
use mundis_clap_utils::fee_payer::{fee_payer_arg, FEE_PAYER_ARG};
use mundis_clap_utils::input_parsers::{pubkey_of, pubkey_of_signer, signer_of};
use mundis_clap_utils::input_validators::{is_amount, is_amount_or_all, is_parsable, is_valid_pubkey, is_valid_signer};
use mundis_clap_utils::keypair::{CliSignerInfo, CliSigners, DefaultSigner, pubkey_from_path, signer_from_path, SignerIndex};
use mundis_clap_utils::memo::{memo_arg, MEMO_ARG};
use mundis_clap_utils::nonce::{NONCE_ARG, NONCE_AUTHORITY_ARG, NonceArgs};
use mundis_clap_utils::offline::{BLOCKHASH_ARG, DUMP_TRANSACTION_MESSAGE, OfflineArgs, SIGN_ONLY_ARG};
use mundis_cli_config::Config;
use mundis_cli_output::{return_signers_with_config, ReturnSignersConfig};
use mundis_client::blockhash_query::BlockhashQuery;
use mundis_client::nonce_utils;
use mundis_client::rpc_client::RpcClient;
use mundis_memo_program::memo_instruction;
use mundis_remote_wallet::remote_wallet::RemoteWalletManager;
use mundis_sdk::message::Message;
use mundis_sdk::native_token::lamports_to_mun;
use mundis_sdk::pubkey::Pubkey;
use mundis_sdk::signature::Keypair;
use mundis_sdk::signer::Signer;
use mundis_sdk::system_instruction;
use mundis_sdk::system_instruction::SystemError;
use mundis_sdk::transaction::Transaction;
use mundis_token_program::native_mint;
use mundis_token_program::state::Mint;
use mundis_token_program::token_instruction::{initialize_mint, MAX_SIGNERS, MIN_SIGNERS};
use crate::cli::{CliCommand, CliCommandInfo, CliConfig, CliError, log_instruction_custom_error, ProcessResult};
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

pub fn parse_create_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let decimals = value_t_or_exit!(matches, "decimals", u8);
    let blockhash_query = BlockhashQuery::new_from_matches(matches);
    let sign_only = matches.is_present(SIGN_ONLY_ARG.name);
    let dump_transaction_message = matches.is_present(DUMP_TRANSACTION_MESSAGE.name);
    let no_wait = matches.is_present("no_wait");

    let mint_authority = pubkey_or_default(matches, "mint_authority", default_signer, wallet_manager);
    let (fee_payer, fee_payer_pubkey) = signer_of(matches, FEE_PAYER_ARG.name, wallet_manager)?;
    let mut bulk_signers = vec![fee_payer];

    let nonce_account = pubkey_of(matches, NONCE_ARG.name);
    let (nonce_authority, nonce_authority_pubkey) =
        signer_of(matches, NONCE_AUTHORITY_ARG.name, wallet_manager)?;
    if nonce_account.is_some() {
        bulk_signers.push(nonce_authority);
    }

    let multisig_signers = signers_of(matches, MULTISIG_SIGNER_ARG.name, wallet_manager)
        .unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            exit(1);
        });

    if let Some(mut multisig_signers) = multisig_signers {
        multisig_signers.sort_by(|(_, lp), (_, rp)| lp.cmp(rp));
        let (signers, pubkeys): (Vec<_>, Vec<_>) = multisig_signers.into_iter().unzip();
        bulk_signers.extend(signers);
    }

    let (token_signer, token) = signer_of(matches, "token_keypair", wallet_manager)
        .map(|signer| {
            if let Some(s) = signer.0 {
                let pubkey = s.pubkey();
                (s, pubkey)
            } else {
                let keypair = Keypair::new();
                let pubkey = keypair.pubkey();
                (Box::new(keypair) as Box<dyn Signer>, pubkey)
            }
        })?;

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::CreateToken {
            token,
            authority: mint_authority,
            decimals,
            enable_freeze: matches.is_present("enable_freeze"),
            memo: matches.value_of(MEMO_ARG.name).map(String::from),
            fee_payer: signer_info.index_of(fee_payer_pubkey).unwrap(),
            blockhash_query,
            nonce_account,
            nonce_authority: signer_info.index_of(nonce_authority_pubkey).unwrap(),
            sign_only,
            dump_transaction_message,
            no_wait
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
    fee_payer: SignerIndex,
    blockhash_query: &BlockhashQuery,
    nonce_account: Option<&Pubkey>,
    nonce_authority: SignerIndex,
    sign_only: bool,
    dump_transaction_message: bool,
    no_wait: bool,
) -> ProcessResult {
    let freeze_authority_pubkey = if enable_freeze { Some(authority) } else { None };
    let recent_blockhash = blockhash_query.get_blockhash(rpc_client, config.commitment)?;
    let nonce_authority = config.signers[nonce_authority];
    let fee_payer = config.signers[fee_payer];

    let minimum_balance_for_rent_exemption = if !sign_only {
        rpc_client.get_minimum_balance_for_rent_exemption(Mint::packed_len())?
    } else {
        0
    };

    let instructions = vec![
        system_instruction::create_account(
            &fee_payer.pubkey(),
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

    // handle tx
    let message = if let Some(nonce_account) = &nonce_account {
        Message::new_with_nonce(
            instructions,
            Some(&fee_payer.pubkey()),
            nonce_account,
            &nonce_authority.pubkey(),
        )
    } else {
        Message::new(&instructions, Some(&fee_payer.pubkey()))
    };

    let message_fee = rpc_client.get_fee_for_message(&message)
        .unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            exit(1);
        });

    if !sign_only {
        check_fee_payer_balance(
            rpc_client,
            &fee_payer.pubkey(),
            minimum_balance_for_rent_exemption + message_fee,
        )?;
    }

    let mut tx = Transaction::new_unsigned(message);
    if sign_only {
        tx.try_partial_sign(&config.signers, recent_blockhash)?;
        return return_signers_with_config(
            &tx,
            &config.output_format,
            &ReturnSignersConfig {
                dump_transaction_message,
            },
        )
    } else {
        if let Some(nonce_account) = &nonce_account {
            let nonce_account = nonce_utils::get_account_with_commitment(
                rpc_client,
                nonce_account,
                config.commitment,
            )?;
            check_nonce_account(&nonce_account, &nonce_authority.pubkey(), &recent_blockhash)?;
        }

        tx.try_sign(&config.signers, recent_blockhash)?;
        let result = if no_wait {
            rpc_client.send_transaction(&tx)
        } else {
            rpc_client.send_and_confirm_transaction_with_spinner(&tx)
        };
        log_instruction_custom_error::<SystemError>(result, config)
    }
}

pub fn parse_create_token_account_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::CreateTokenAccount,
        signers: CliSigners::new(),
    })
}

pub fn process_create_token_account_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_create_multisig_token_account_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::CreateMultisigToken,
        signers: CliSigners::new(),
    })
}

pub fn process_create_multisig_token_account_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
) -> ProcessResult {
    Ok("ok".to_string())
}

pub fn parse_authorize_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    Ok(CliCommandInfo {
        command: CliCommand::AuthorizeToken,
        signers: CliSigners::new(),
    })
}

pub fn process_authorize_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
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

fn new_throwaway_signer() -> (Option<Box<dyn Signer>>, Option<Pubkey>) {
    let keypair = Keypair::new();
    let pubkey = keypair.pubkey();
    (Some(Box::new(keypair) as Box<dyn Signer>), Some(pubkey))
}

pub fn check_fee_payer_balance(rpc_client: &RpcClient, fee_payer: &Pubkey, required_balance: u64) -> Result<(), Box<dyn std::error::Error>> {
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