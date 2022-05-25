use std::collections::{BTreeMap, HashMap};
use std::collections::btree_map::Entry;
use std::process::exit;
use std::str::FromStr;
use std::sync::Arc;

use clap::{App, AppSettings, Arg, ArgMatches, SubCommand, value_t_or_exit};

use mundis_account_decoder::parse_token::{TokenAccountType, UiAccountState};
use mundis_account_decoder::UiAccountData;
use mundis_clap_utils::{ArgConstant, offline};
use mundis_clap_utils::fee_payer::{fee_payer_arg, FEE_PAYER_ARG};
use mundis_clap_utils::input_parsers::{pubkey_of, pubkey_of_signer, pubkeys_of_multiple_signers, signer_of, signer_of_or_else, signer_or_default, value_of};
use mundis_clap_utils::input_validators::{is_amount, is_amount_or_all, is_parsable, is_valid_pubkey, is_valid_signer};
use mundis_clap_utils::keypair::{CliSigners, DefaultSigner, pubkey_from_path, signer_from_path};
use mundis_clap_utils::memo::{memo_arg, MEMO_ARG};
use mundis_clap_utils::nonce::{NONCE_ARG, NONCE_AUTHORITY_ARG, NonceArgs};
use mundis_clap_utils::offline::{BLOCKHASH_ARG, OfflineArgs, SIGN_ONLY_ARG};
use mundis_cli_output::{CliMint, CliMultisig, CliSignature, CliSignOnlyData, CliTokenAccount, CliTokenAccounts, CliTokenAmount, CliWalletAddress, OutputFormat, return_signers_data, ReturnSignersConfig, UnsupportedAccount};
use mundis_client::rpc_client::RpcClient;
use mundis_client::rpc_request::TokenAccountsFilter;
use mundis_client::rpc_response::RpcKeyedAccount;
use mundis_remote_wallet::remote_wallet::RemoteWalletManager;
use mundis_sdk::{system_instruction, system_program};
use mundis_sdk::instruction::Instruction;
use mundis_sdk::message::Message;
use mundis_sdk::native_token::{lamports_to_mdis, mdis_to_lamports};
use mundis_sdk::pubkey::Pubkey;
use mundis_sdk::signature::Keypair;
use mundis_sdk::signer::Signer;
use mundis_sdk::transaction::Transaction;
use mundis_token_account_program::get_associated_token_address;
use mundis_token_account_program::token_account_instruction::create_associated_token_account;
use mundis_token_program::native_mint;
use mundis_token_program::state::{MAX_NAME_LENGTH, MAX_SYMBOL_LENGTH, Mint, Multisig, TokenAccount};
use mundis_token_program::token_instruction::{approve, approve_checked, AuthorityType, burn, burn_checked, close_account, freeze_account, initialize_account, initialize_mint, initialize_multisig, MAX_SIGNERS, MIN_SIGNERS, mint_to, mint_to_checked, revoke, set_authority, sync_native, thaw_account, transfer, transfer_checked};

use crate::cli::{CliCommand, CliCommandInfo, CliConfig, CliError, create_tx_info, ProcessResult, TxInfo};
use crate::memo::WithMemo;

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
                            Arg::with_name("name")
                                .long("name")
                                .validator(is_token_name_field)
                                .value_name("TOKEN_NAME")
                                .takes_value(true)
                                .index(1)
                                .help(&formatcp!("Name of the token (max. {} chars)", MAX_NAME_LENGTH)),
                        )
                        .arg(
                            Arg::with_name("symbol")
                                .long("symbol")
                                .validator(is_token_symbol_field)
                                .value_name("TOKEN_SYMBOL")
                                .takes_value(true)
                                .index(2)
                                .help(&formatcp!("Symbol of the token (max. {} chars)", MAX_SYMBOL_LENGTH)),
                        )
                        .arg(
                            Arg::with_name("token_keypair")
                                .value_name("TOKEN_KEYPAIR")
                                .validator(is_valid_signer)
                                .takes_value(true)
                                .index(3)
                                .help(
                                    "Specify the token keypair. \
                             This may be a keypair file or the ASK keyword. \
                             [default: randomly generated keypair]"
                                ),
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
                    SubCommand::with_name("unfreeze")
                        .about("Unfreeze a token account")
                        .arg(
                            Arg::with_name("account")
                                .validator(is_valid_pubkey)
                                .value_name("TOKEN_ACCOUNT_ADDRESS")
                                .takes_value(true)
                                .index(1)
                                .required(true)
                                .help("The address of the token account to unfreeze"),
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
                        .about("Wrap native MUNDIS in a MUNDIS token account")
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
                        .about("Sync a native MUNDIS token account to its underlying lamports")
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

pub fn add_default_signers<'a>(
    matches: &ArgMatches<'_>,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
    bulk_signers: &mut Vec<Option<Box<dyn Signer>>>,
) -> Result<(Option<Pubkey>, Option<Pubkey>, Option<Pubkey>, Vec<Pubkey>), CliError> {
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
    let mut multisigner_ids = Vec::new();
    let multisig_signers = signers_of(matches, MULTISIG_SIGNER_ARG.name, wallet_manager)
        .unwrap_or_else(|e| {
            eprintln!("error: {}", e);
            exit(1);
        });

    if let Some(mut multisig_signers) = multisig_signers {
        multisig_signers.sort_by(|(_, lp), (_, rp)| lp.cmp(rp));
        let (signers, pubkeys): (Vec<_>, Vec<_>) = multisig_signers.into_iter().unzip();
        bulk_signers.extend(signers);
        multisigner_ids = pubkeys;
    }
    let multisigner_pubkeys = multisigner_ids.iter().map(|pk| pk.unwrap()).collect::<Vec<_>>();

   Ok((fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys))
}

pub fn parse_create_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, _) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let name = value_t_or_exit!(matches, "name", String);
    let symbol = value_t_or_exit!(matches, "symbol", String);
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
            name,
            symbol,
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
    name: &String,
    symbol: &String,
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
            name,
            symbol,
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
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, _) =
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
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, _) =
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
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys) =
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

    let (authority_signer, authority)  =
        signer_or_default(matches, "authority", default_signer, wallet_manager)?;
    bulk_signers.push(authority_signer);

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
            authority: authority.unwrap(),
            new_authority,
            force_authorize,
            multisigner_pubkeys,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_authorize_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    account: Pubkey,
    authority: Pubkey,
    authority_type: AuthorityType,
    new_authority: Option<Pubkey>,
    force_authorize: bool,
    multisigner_pubkeys: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    let auth_str = match authority_type {
        AuthorityType::MintTokens => "mint authority",
        AuthorityType::FreezeAccount => "freeze authority",
        AuthorityType::AccountOwner => "owner",
        AuthorityType::CloseAccount => "close authority",
    };

    let previous_authority = if !tx_info.sign_only {
        let target_account = rpc_client.get_account(&account)?;
        if let Ok(mint) = Mint::unpack(&target_account.data) {
            match authority_type {
                AuthorityType::AccountOwner | AuthorityType::CloseAccount => Err(format!(
                    "Authority type `{}` not supported for SPL Token mints",
                    auth_str
                )),
                AuthorityType::MintTokens => Ok(mint.mint_authority),
                AuthorityType::FreezeAccount => Ok(mint.freeze_authority),
            }
        } else if let Ok(token_account) = TokenAccount::unpack(&target_account.data) {
            let check_associated_token_account = || -> Result<(),  Box<dyn std::error::Error>> {
                let maybe_associated_token_account =
                    get_associated_token_address(&token_account.owner, &token_account.mint);
                if account == maybe_associated_token_account
                    && !force_authorize
                    && Some(authority) != new_authority
                {
                    return Err(format!(
                        "Error: attempting to change the `{}` of an associated token account",
                        auth_str
                    )
                        .into())
                } else {
                    Ok(())
                }
            };

            match authority_type {
                AuthorityType::MintTokens | AuthorityType::FreezeAccount => Err(format!(
                    "Authority type `{}` not supported for SPL Token accounts",
                    auth_str
                )),
                AuthorityType::AccountOwner => {
                    check_associated_token_account()?;
                    Ok(Some(token_account.owner))
                }
                AuthorityType::CloseAccount => {
                    check_associated_token_account()?;
                    Ok(Some(
                        token_account.close_authority.unwrap_or(token_account.owner),
                    ))
                }
            }
        }  else {
            Err("Unsupported account data format".to_string())
        }?
    } else {
        None
    };

    println_display(
        config,
        format!(
            "Updating {}\n  Current {}: {}\n  New {}: {}",
            account,
            auth_str,
            previous_authority
                .map(|pubkey| pubkey.to_string())
                .unwrap_or_else(|| "disabled".to_string()),
            auth_str,
            new_authority
                .map(|pubkey| pubkey.to_string())
                .unwrap_or_else(|| "disabled".to_string())
        ),
    );

    let instructions = vec![set_authority(
        &mundis_token_program::id(),
        &account,
        new_authority.as_ref(),
        authority_type,
        &authority,
        multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice()
    )?];

    let tx_return = handle_tx(
        rpc_client,
        config,
        0,
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

pub fn parse_transfer_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let token = pubkey_of_signer(matches, "token", wallet_manager)
        .unwrap()
        .unwrap();
    let amount = match matches.value_of("amount").unwrap() {
        "ALL" => None,
        amount => Some(amount.parse::<f64>().unwrap()),
    };
    let recipient = pubkey_of_signer(matches, "recipient", wallet_manager)
        .unwrap()
        .unwrap();
    let sender = pubkey_of_signer(matches, "from", wallet_manager).unwrap();

    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let (owner_signer, owner) =
        signer_or_default(matches, "owner", default_signer, wallet_manager)?;
    bulk_signers.push(owner_signer);

    let mint_decimals = value_of::<u8>(matches, MINT_DECIMALS_ARG.name);
    let fund_recipient = matches.is_present("fund_recipient");
    let allow_unfunded_recipient = matches.is_present("allow_empty_recipient")
        || matches.is_present("allow_unfunded_recipient");

    let recipient_is_ata_owner = matches.is_present("recipient_is_ata_owner");
    let use_unchecked_instruction = matches.is_present("use_unchecked_instruction");
    let memo =matches.value_of(MEMO_ARG.name).map(String::from);

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::TransferToken {
            token,
            ui_amount: amount,
            recipient,
            sender,
            sender_owner: owner.unwrap(),
            allow_unfunded_recipient,
            fund_recipient,
            mint_decimals,
            recipient_is_ata_owner,
            use_unchecked_instruction,
            memo,
            multisigner_pubkeys,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers
    })
}

pub fn process_transfer_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    token: Pubkey,
    ui_amount: Option<f64>,
    recipient: Pubkey,
    sender: Option<Pubkey>,
    sender_owner: Pubkey,
    allow_unfunded_recipient: bool,
    fund_recipient: bool,
    mint_decimals: Option<u8>,
    recipient_is_ata_owner: bool,
    use_unchecked_instruction: bool,
    memo: Option<&String>,
    multisigner_pubkeys: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    let sender = if let Some(sender) = sender {
        sender
    } else {
        get_associated_token_address(&sender_owner, &token)
    };
    let (mint_pubkey, decimals) = resolve_mint_info(rpc_client, tx_info.sign_only, &sender, Some(token), mint_decimals)?;
    let maybe_transfer_balance =
        ui_amount.map(|ui_amount| mundis_token_program::ui_amount_to_amount(ui_amount, decimals));
    let transfer_balance = if !tx_info.sign_only {
        let sender_token_amount = rpc_client.get_token_account_balance(&sender)
            .map_err(|err| {
                format!(
                    "Error: Failed to get token balance of sender address {}: {}",
                    sender, err
                )
            })?;
        let sender_balance = sender_token_amount.amount.parse::<u64>().map_err(|err| {
            format!(
                "Token account {} balance could not be parsed: {}",
                sender, err
            )
        })?;

        let transfer_balance = maybe_transfer_balance.unwrap_or(sender_balance);
        println_display(
            config,
            format!(
                "Transfer {} tokens\n  Sender: {}\n  Recipient: {}",
                mundis_token_program::amount_to_ui_amount(transfer_balance, decimals),
                sender,
                recipient
            ),
        );

        if transfer_balance > sender_balance {
            return Err(format!(
                "Error: Sender has insufficient funds, current balance is {}",
                sender_token_amount.real_number_string_trimmed()
            )
                .into());
        }
        transfer_balance
    } else {
        maybe_transfer_balance.unwrap()
    };

    let mut instructions: Vec<Instruction> = vec![];

    let mut recipient_token_account = recipient;
    let mut minimum_balance_for_rent_exemption = 0;

    let recipient_is_token_account = if !tx_info.sign_only {
        let recipient_account_info = rpc_client
            .get_account_with_commitment(&recipient, config.commitment)?
            .value
            .map(|account| account.owner == mundis_token_program::id() && account.data.len() == TokenAccount::packed_len());

        if recipient_account_info.is_none() && !allow_unfunded_recipient {
            return Err("Error: The recipient address is not funded. \
                                    Add `--allow-unfunded-recipient` to complete the transfer \
                                   "
                .into());
        }

        recipient_account_info.unwrap_or(false)
    } else {
        !recipient_is_ata_owner
    };

    if !recipient_is_token_account {
        recipient_token_account = get_associated_token_address(&recipient, &mint_pubkey);
        println_display(
            config,
            format!(
                "  Recipient associated token account: {}",
                recipient_token_account
            ),
        );

        let needs_funding = if !tx_info.sign_only {
            if let Some(recipient_token_account_data) = rpc_client
                .get_account_with_commitment(&recipient_token_account, config.commitment)?
                .value
            {
                if recipient_token_account_data.owner == system_program::id() {
                    true
                } else if recipient_token_account_data.owner == mundis_token_program::id() {
                    false
                } else {
                    return Err(
                        format!("Error: Unsupported recipient address: {}", recipient).into(),
                    );
                }
            } else {
                true
            }
        } else {
            fund_recipient
        };

        if needs_funding {
            if fund_recipient {
                if !tx_info.sign_only {
                    minimum_balance_for_rent_exemption += rpc_client.get_minimum_balance_for_rent_exemption(TokenAccount::packed_len())?;
                    println_display(
                        config,
                        format!(
                            "  Funding recipient: {} ({} MUNDIS)",
                            recipient_token_account,
                            lamports_to_mdis(minimum_balance_for_rent_exemption)
                        ),
                    );
                }
                instructions.push(create_associated_token_account(
                    &config.signers[tx_info.fee_payer].pubkey(),
                    &recipient,
                    &mint_pubkey,
                ));
            } else {
                return Err(
                    "Error: Recipient's associated token account does not exist. \
                                    Add `--fund-recipient` to fund their account"
                        .into(),
                );
            }
        }
    }

    if use_unchecked_instruction {
        instructions.push(transfer(
            &mundis_token_program::id(),
            &sender,
            &recipient_token_account,
            &sender_owner,
            multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
            transfer_balance,
        )?);
    } else {
        instructions.push(transfer_checked(
            &mundis_token_program::id(),
            &sender,
            &mint_pubkey,
            &recipient_token_account,
            &sender_owner,
            multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
            transfer_balance,
            decimals,
        )?);
    }

    let tx_return = handle_tx(
        rpc_client,
        config,
        minimum_balance_for_rent_exemption,
        instructions.with_memo(memo),
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

pub fn parse_burn_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let source = pubkey_of_signer(matches, "source", wallet_manager)
        .unwrap()
        .unwrap();

    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let (owner_signer, owner) =
        signer_or_default(matches, "owner", default_signer, wallet_manager)?;
    bulk_signers.push(owner_signer);

    let amount = value_t_or_exit!(matches, "amount", f64);
    let mint_address = pubkey_of_signer(matches, MINT_ADDRESS_ARG.name, wallet_manager).unwrap();
    let mint_decimals = value_of::<u8>(matches, MINT_DECIMALS_ARG.name);
    let use_unchecked_instruction = matches.is_present("use_unchecked_instruction");
    let memo = matches.value_of(MEMO_ARG.name).map(String::from);

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::BurnToken {
            source,
            source_owner: owner.unwrap(),
            ui_amount: amount,
            mint_address,
            mint_decimals,
            use_unchecked_instruction,
            memo,
            multisigner_pubkeys,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_burn_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    source: Pubkey,
    source_owner: Pubkey,
    ui_amount: f64,
    mint_address: Option<Pubkey>,
    mint_decimals: Option<u8>,
    use_unchecked_instruction: bool,
    memo: Option<&String>,
    multisigner_pubkeys: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    println_display(
        config,
        format!("Burn {} tokens\n  Source: {}", ui_amount, source),
    );

    let (mint_pubkey, decimals) = resolve_mint_info(rpc_client, tx_info.sign_only, &source, mint_address, mint_decimals)?;
    let amount = mundis_token_program::ui_amount_to_amount(ui_amount, decimals);
    let instructions = if use_unchecked_instruction {
        vec![burn(
            &mundis_token_program::id(),
            &source,
            &mint_pubkey,
            &source_owner,
            &multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
            amount,
        )?]
    } else {
        vec![burn_checked(
            &mundis_token_program::id(),
            &source,
            &mint_pubkey,
            &source_owner,
            &multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
            amount,
            decimals,
        )?]
    };

    let tx_return = handle_tx(
        rpc_client,
        config,
        0,
        instructions.with_memo(memo),
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

pub fn parse_mint_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let (mint_authority_signer, mint_authority) =
        signer_or_default(matches, "mint_authority", default_signer, wallet_manager)?;
    bulk_signers.push(mint_authority_signer);


    let token = pubkey_of_signer(matches, "token", wallet_manager)
        .unwrap()
        .unwrap();

    let amount = value_t_or_exit!(matches, "amount", f64);
    let recipient = associated_token_address_or_override(
        matches,
        "recipient",
        default_signer,
        wallet_manager,
    );
    let mint_decimals = value_of::<u8>(matches, MINT_DECIMALS_ARG.name);
    let use_unchecked_instruction = matches.is_present("use_unchecked_instruction");

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::MintToken {
            token,
            ui_amount: amount,
            recipient,
            mint_decimals,
            mint_authority: mint_authority.unwrap(),
            use_unchecked_instruction,
            multisigner_pubkeys,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_mint_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    token: Pubkey,
    ui_amount: f64,
    recipient: Pubkey,
    mint_decimals: Option<u8>,
    mint_authority: Pubkey,
    use_unchecked_instruction: bool,
    multisigner_pubkeys: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    println_display(
        config,
        format!(
            "Minting {} tokens\n  Token: {}\n  Recipient: {}",
            ui_amount, token, recipient
        ),
    );

    let (_, decimals) = resolve_mint_info(rpc_client, tx_info.sign_only, &recipient, None, mint_decimals)?;
    let amount = mundis_token_program::ui_amount_to_amount(ui_amount, decimals);

    let instructions = if use_unchecked_instruction {
        vec![mint_to(
            &mundis_token_program::id(),
            &token,
            &recipient,
            &mint_authority,
           multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
            amount,
        )?]
    } else {
        vec![mint_to_checked(
            &mundis_token_program::id(),
            &token,
            &recipient,
            &mint_authority,
            multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
            amount,
            decimals,
        )?]
    };

    let tx_return = handle_tx(
        rpc_client,
        config,
        0,
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

pub fn parse_freeze_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let (freeze_authority_signer, freeze_authority) =
        signer_or_default(matches, "freeze_authority", default_signer, wallet_manager)?;
    bulk_signers.push(freeze_authority_signer);

    let account = pubkey_of_signer(matches, "account", wallet_manager)
        .unwrap()
        .unwrap();
    let mint_address = pubkey_of_signer(matches, MINT_ADDRESS_ARG.name, wallet_manager).unwrap();

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::FreezeTokenAccount {
            account,
            mint_address,
            freeze_authority: freeze_authority.unwrap(),
            multisigner_pubkeys,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_freeze_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    account: Pubkey,
    mint_address: Option<Pubkey>,
    freeze_authority: Pubkey,
    multisigner_pubkeys: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    let (token, _) = resolve_mint_info(rpc_client, tx_info.sign_only, &account, mint_address, None)?;

    println_display(
        config,
        format!("Freezing account: {}\n  Token: {}", account, token),
    );

    let instructions = vec![freeze_account(
        &mundis_token_program::id(),
        &account,
        &token,
        &freeze_authority,
        multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
    )?];

    let tx_return = handle_tx(
        rpc_client,
        config,
        0,
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

pub fn parse_unfreeze_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let (freeze_authority_signer, freeze_authority) =
        signer_or_default(matches, "freeze_authority", default_signer, wallet_manager)?;
    bulk_signers.push(freeze_authority_signer);

    let account = pubkey_of_signer(matches, "account", wallet_manager)
        .unwrap()
        .unwrap();
    let mint_address =
        pubkey_of_signer(matches, MINT_ADDRESS_ARG.name, wallet_manager).unwrap();

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::ThawTokenAccount {
            account,
            mint_address,
            freeze_authority: freeze_authority.unwrap(),
            multisigner_pubkeys,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_unfreeze_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    account: Pubkey,
    mint_address: Option<Pubkey>,
    freeze_authority: Pubkey,
    multisigner_pubkeys: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    let (token, _) = resolve_mint_info(rpc_client, tx_info.sign_only, &account, mint_address, None)?;

    println_display(
        config,
        format!("Unfreezing account: {}\n  Token: {}", account, token),
    );

    let instructions = vec![thaw_account(
        &mundis_token_program::id(),
        &account,
        &token,
        &freeze_authority,
        multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
    )?];

    let tx_return = handle_tx(
        rpc_client,
        config,
        0,
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

pub fn parse_wrap_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, _) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let amount = value_t_or_exit!(matches, "amount", f64);
    let account = if matches.is_present("create_aux_account") {
        let (signer, account) = new_throwaway_signer();
        bulk_signers.push(signer);
        account
    } else {
        // No need to add a signer when creating an associated token account
        None
    };

    let (wallet_signer, wallet_address) =
        signer_or_default(matches, "wallet_keypair", default_signer, wallet_manager)?;
    bulk_signers.push(wallet_signer);

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::WrapToken {
            mdis: amount,
            wallet_address: wallet_address.unwrap(),
            wrapped_sol_account: account,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_wrap_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    mdis: f64,
    wallet_address: Pubkey,
    wrapped_sol_account: Option<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    let lamports = mdis_to_lamports(mdis);
    let instructions = if let Some(wrapped_sol_account) = wrapped_sol_account {
        println_display(
            config,
            format!("Wrapping {} MUNDIS into {}", mdis, wrapped_sol_account),
        );
        vec![
            system_instruction::create_account(
                &wallet_address,
                &wrapped_sol_account,
                lamports,
                TokenAccount::packed_len() as u64,
                &mundis_token_program::id(),
            ),
            initialize_account(
                &mundis_token_program::id(),
                &wrapped_sol_account,
                &native_mint::id(),
                &wallet_address,
            )?,
        ]
    }  else {
        let account = get_associated_token_address(&wallet_address, &native_mint::id());

        if !tx_info.sign_only {
            if let Some(account_data) = rpc_client
                .get_account_with_commitment(&account, config.commitment)?
                .value
            {
                if account_data.owner != system_program::id() {
                    return Err(format!("Error: Account already exists: {}", account).into());
                }
            }
        }

        println_display(config, format!("Wrapping {} MUNDIS into {}", mdis, account));
        vec![
            system_instruction::transfer(&wallet_address, &account, lamports),
            create_associated_token_account(&config.signers[tx_info.fee_payer].pubkey(), &wallet_address, &native_mint::id()),
        ]
    };

    if !tx_info.sign_only {
        check_wallet_balance(rpc_client, &wallet_address, lamports)?;
    }

    let tx_return = handle_tx(
        rpc_client,
        config,
        0,
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

pub fn parse_unwrap_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let (wallet_signer, wallet_address) =
        signer_or_default(matches, "wallet_keypair", default_signer, wallet_manager)?;
    bulk_signers.push(wallet_signer);

    let address = pubkey_of_signer(matches, "address", wallet_manager).unwrap();

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::UnwrapToken {
            wallet_address: wallet_address.unwrap(),
            address,
            multisigner_pubkeys,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_unwrap_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    wallet_address: Pubkey,
    address: Option<Pubkey>,
    multisigner_pubkeys: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    let use_associated_account = address.is_none();
    let address = address
        .unwrap_or_else(|| get_associated_token_address(&wallet_address, &native_mint::id()));
    println_display(config, format!("Unwrapping {}", address));

    if !tx_info.sign_only {
        let lamports = rpc_client.get_balance(&address)?;
        if lamports == 0 {
            if use_associated_account {
                return Err("No wrapped MUNDIS in associated account; did you mean to specify an auxiliary address?".to_string().into());
            } else {
                return Err(format!("No wrapped MUNDIS in {}", address).into());
            }
        }
        println_display(
            config,
            format!("  Amount: {} MUNDIS", lamports_to_mdis(lamports)),
        );
    }
    println_display(config, format!("  Recipient: {}", &wallet_address));

    let instructions = vec![close_account(
        &mundis_token_program::id(),
        &address,
        &wallet_address,
        &wallet_address,
        &multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
    )?];

    let tx_return = handle_tx(
        rpc_client,
        config,
        0,
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

pub fn parse_approve_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let (owner_signer, owner_address) =
        signer_or_default(matches, "owner", default_signer, wallet_manager)?;
    bulk_signers.push(owner_signer);

    let account = pubkey_of_signer(matches, "account", wallet_manager)
        .unwrap()
        .unwrap();
    let amount = value_t_or_exit!(matches, "amount", f64);
    let delegate = pubkey_of_signer(matches, "delegate", wallet_manager)
        .unwrap()
        .unwrap();
    let mint_address =
        pubkey_of_signer(matches, MINT_ADDRESS_ARG.name, wallet_manager).unwrap();
    let mint_decimals = value_of::<u8>(matches, MINT_DECIMALS_ARG.name);
    let use_unchecked_instruction = matches.is_present("use_unchecked_instruction");

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::ApproveToken {
            account,
            owner: owner_address.unwrap(),
            ui_amount: amount,
            delegate,
            mint_address,
            mint_decimals,
            use_unchecked_instruction,
            multisigner_pubkeys,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_approve_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    account: Pubkey,
    owner: Pubkey,
    ui_amount: f64,
    delegate: Pubkey,
    mint_address: Option<Pubkey>,
    mint_decimals: Option<u8>,
    use_unchecked_instruction: bool,
    multisigner_pubkeys: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    println_display(
        config,
        format!(
            "Approve {} tokens\n  Account: {}\n  Delegate: {}",
            ui_amount, account, delegate
        ),
    );

    let (mint_pubkey, decimals) = resolve_mint_info(rpc_client, tx_info.sign_only, &account, mint_address, mint_decimals)?;
    let amount = mundis_token_program::ui_amount_to_amount(ui_amount, decimals);

    let instructions = if use_unchecked_instruction {
        vec![approve(
            &mundis_token_program::id(),
            &account,
            &delegate,
            &owner,
            &multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
            amount,
        )?]
    } else {
        vec![approve_checked(
            &mundis_token_program::id(),
            &account,
            &mint_pubkey,
            &delegate,
            &owner,
            &multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
            amount,
            decimals,
        )?]
    };

    let tx_return = handle_tx(
        rpc_client,
        config,
        0,
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

pub fn parse_revoke_token_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let (owner_signer, owner_address) =
        signer_or_default(matches, "owner", default_signer, wallet_manager)?;
    bulk_signers.push(owner_signer);

    let account = pubkey_of_signer(matches, "account", wallet_manager)
        .unwrap()
        .unwrap();
    let delegate_address =
        pubkey_of_signer(matches, DELEGATE_ADDRESS_ARG.name, wallet_manager)
            .unwrap();

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::RevokeToken {
            account,
            owner: owner_address.unwrap(),
            delegate: delegate_address,
            multisigner_pubkeys,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_revoke_token_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    account: Pubkey,
    owner: Pubkey,
    delegate: Option<Pubkey>,
    multisigner_pubkeys: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    let delegate = if !tx_info.sign_only {
        let source_account = rpc_client
            .get_token_account(&account)?
            .ok_or_else(|| format!("Could not find token account {}", account))?;

        if let Some(string) = source_account.delegate {
            Some(Pubkey::from_str(&string)?)
        } else {
            None
        }
    } else {
        delegate
    };

    if let Some(delegate) = delegate {
        println_display(
            config,
            format!(
                "Revoking approval\n  Account: {}\n  Delegate: {}",
                account, delegate
            ),
        );
    } else {
        return Err(format!("No delegate on account {}", account).into());
    }

    let instructions = vec![revoke(
        &mundis_token_program::id(),
        &account,
        &owner,
        &multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
    )?];

    let tx_return = handle_tx(
        rpc_client,
        config,
        0,
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

pub fn parse_close_token_account_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let (close_authority_signer, close_authority) =
        signer_or_default(matches, "close_authority", default_signer, wallet_manager)?;
    bulk_signers.push(close_authority_signer);

    let address = associated_token_address_or_override(
        matches,
        "address",
        default_signer,
        wallet_manager,
    );
    let recipient = pubkey_or_default(matches, "recipient", default_signer, wallet_manager);

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::CloseTokenAccount {
            account: address,
            close_authority: close_authority.unwrap(),
            recipient,
            multisigner_pubkeys,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_close_token_account_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    account: Pubkey,
    close_authority: Pubkey,
    recipient: Pubkey,
    multisigner_pubkeys: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    if !tx_info.sign_only {
        let source_account = rpc_client
            .get_token_account(&account)?
            .ok_or_else(|| format!("Could not find token account {}", account))?;
        let source_amount = source_account
            .token_amount
            .amount
            .parse::<u64>()
            .map_err(|err| {
                format!(
                    "Token account {} balance could not be parsed: {}",
                    account, err
                )
            })?;

        if !source_account.is_native && source_amount > 0 {
            return Err(format!(
                "Account {} still has {} tokens; empty the account in order to close it.",
                account,
                source_account.token_amount.real_number_string_trimmed()
            )
                .into());
        }
    }

    let instructions = vec![close_account(
        &mundis_token_program::id(),
        &account,
        &recipient,
        &close_authority,
        &multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
    )?];

    let tx_return = handle_tx(
        rpc_client,
        config,
        0,
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

pub fn parse_token_balance_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let address = associated_token_address_or_override(
        matches,
        "address",
        default_signer,
        wallet_manager,
    );

    Ok(CliCommandInfo {
        command: CliCommand::GetTokenAccountBalance {
            address
        },
        signers: vec![],
    })
}

pub fn process_token_balance_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    address: Pubkey
) -> ProcessResult {
    let balance = rpc_client
        .get_token_account_balance(&address)
        .map_err(|_| format!("Could not find token account {}", address))?;
    let cli_token_amount = CliTokenAmount { amount: balance };
    Ok(config.output_format.formatted_string(&cli_token_amount))
}

pub fn parse_token_supply_command(
    matches: &ArgMatches<'_>,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let address = pubkey_of_signer(matches, "address", wallet_manager)
        .unwrap()
        .unwrap();

    Ok(CliCommandInfo {
        command: CliCommand::GetTokenSupply {
            address
        },
        signers: vec![],
    })
}

pub fn process_token_supply_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    address: Pubkey,
) -> ProcessResult {
    let supply = rpc_client.get_token_supply(&address)?;
    let cli_token_amount = CliTokenAmount { amount: supply };
    Ok(config.output_format.formatted_string(&cli_token_amount))
}

pub fn parse_token_list_accounts_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let token = pubkey_of_signer(matches, "token", wallet_manager).unwrap();
    let owner = pubkey_or_default(matches, "owner", default_signer, wallet_manager);

    Ok(CliCommandInfo {
        command: CliCommand::ListTokenAccounts {
            token,
            owner
        },
        signers: vec![],
    })
}

pub fn process_token_list_accounts_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    token: Option<Pubkey>,
    owner: Pubkey
) -> ProcessResult {
    if let Some(token) = token {
        validate_mint(rpc_client, token)?;
    }

    let accounts = rpc_client.get_token_accounts_by_owner(
        &owner,
        match token {
            Some(token) => TokenAccountsFilter::Mint(token),
            None => TokenAccountsFilter::ProgramId(mundis_token_program::id()),
        },
    )?;
    if accounts.is_empty() {
        println!("None");
        return Ok("".to_string());
    }

    let (mint_accounts, unsupported_accounts, max_len_balance, includes_aux) =
        sort_and_parse_token_accounts(&owner, accounts);
    let aux_len = if includes_aux { 10 } else { 0 };

    let cli_token_accounts = CliTokenAccounts {
        accounts: mint_accounts
            .into_iter()
            .map(|(_mint, accounts_list)| accounts_list)
            .collect(),
        unsupported_accounts,
        max_len_balance,
        aux_len,
        token_is_some: token.is_some(),
    };
    Ok(config.output_format.formatted_string(&cli_token_accounts))
}

pub fn parse_token_wallet_address_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let token = pubkey_of_signer(matches, "token", wallet_manager).unwrap();
    let owner = pubkey_or_default(matches, "owner", default_signer, wallet_manager);

    Ok(CliCommandInfo {
        command: CliCommand::GetTokenWalletAddress {
            token,
            owner
        },
        signers: CliSigners::new(),
    })
}

pub fn process_token_wallet_address_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    token: Option<Pubkey>,
    owner: Pubkey
) -> ProcessResult {
    let mut cli_address = CliWalletAddress {
        wallet_address: owner.to_string(),
        ..CliWalletAddress::default()
    };
    if let Some(token) = token {
        validate_mint(rpc_client, token)?;
        let associated_token_address = get_associated_token_address(&owner, &token);
        cli_address.associated_token_address = Some(associated_token_address.to_string());
    }
    Ok(config.output_format.formatted_string(&cli_address))
}

pub fn parse_token_account_info_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let address = associated_token_address_or_override(
        matches,
        "address",
        default_signer,
        wallet_manager,
    );

    Ok(CliCommandInfo {
        command: CliCommand::GetTokenAccountByAddress {
            address
        },
        signers: vec![],
    })
}

pub fn process_token_account_info_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    address: Pubkey
) -> ProcessResult {
    let account = rpc_client
        .get_token_account(&address)
        .map_err(|_| format!("Could not find token account {}", address))?
        .unwrap();
    let mint = Pubkey::from_str(&account.mint).unwrap();
    let owner = Pubkey::from_str(&account.owner).unwrap();
    let is_associated = get_associated_token_address(&owner, &mint) == address;
    let cli_token_account = CliTokenAccount {
        address: address.to_string(),
        is_associated,
        account,
    };
    Ok(config.output_format.formatted_string(&cli_token_account))
}

pub fn parse_token_multisig_info_command(
    matches: &ArgMatches<'_>,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let address = pubkey_of_signer(matches, "address", wallet_manager)
        .unwrap()
        .unwrap();

    Ok(CliCommandInfo {
        command: CliCommand::GetTokenMultisigAccountByAddress {
            address
        },
        signers: vec![],
    })
}

pub fn process_token_multisig_info_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    address: Pubkey
) -> ProcessResult {
    let account = rpc_client.get_account(&address)?;
    let multisig = Multisig::unpack(&account.data)
        .map_err(|_| format!("Not a multisig token account"))?;

    let n = multisig.n as usize;
    assert!(n <= multisig.signers.len());

    let cli_multisig = CliMultisig {
        address: address.to_string(),
        m: multisig.m,
        n: multisig.n,
        signers: multisig
            .signers
            .iter()
            .enumerate()
            .filter_map(|(i, signer)| {
                if i < n {
                    Some(signer.to_string())
                } else {
                    None
                }
            })
            .collect(),
    };
    Ok(config.output_format.formatted_string(&cli_multisig))
}

pub fn parse_token_cleanup_accounts_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let close_empty_associated_accounts =
        matches.is_present("close_empty_associated_accounts");

    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, multisigner_pubkeys) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let (owner_signer, owner_address) =
        signer_or_default(matches, "owner", default_signer, wallet_manager)?;
    bulk_signers.push(owner_signer);

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::CleanupTokenAccounts {
            owner: owner_address.unwrap(),
            close_empty_associated_accounts,
            multisigner_pubkeys,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_token_cleanup_accounts_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    owner: Pubkey,
    close_empty_associated_accounts: bool,
    multisigner_pubkeys: &Vec<Pubkey>,
    tx_info: &TxInfo,
) -> ProcessResult {
    println_display(config, "Fetching token accounts".to_string());
    let accounts = rpc_client
        .get_token_accounts_by_owner(&owner, TokenAccountsFilter::ProgramId(mundis_token_program::id()))?;
    if accounts.is_empty() {
        println_display(config, "Nothing to do".to_string());
        return Ok("".to_string());
    }

    let minimum_balance_for_rent_exemption = if !tx_info.sign_only {
        rpc_client.get_minimum_balance_for_rent_exemption(TokenAccount::LEN)?
    } else {
        0
    };

    let mut accounts_by_token = HashMap::new();
    for keyed_account in accounts {
        if let UiAccountData::Json(parsed_account) = keyed_account.account.data {
            if parsed_account.program == "token" {
                if let Ok(TokenAccountType::Account(ui_token_account)) =
                serde_json::from_value(parsed_account.parsed)
                {
                    let frozen = ui_token_account.state == UiAccountState::Frozen;

                    let token = ui_token_account
                        .mint
                        .parse::<Pubkey>()
                        .unwrap_or_else(|err| panic!("Invalid mint: {:?}", err));
                    let token_account = keyed_account
                        .pubkey
                        .parse::<Pubkey>()
                        .unwrap_or_else(|err| panic!("Invalid token account: {}", err));
                    let token_amount = ui_token_account
                        .token_amount
                        .amount
                        .parse::<u64>()
                        .unwrap_or_else(|err| panic!("Invalid token amount: {}", err));

                    let close_authority = ui_token_account.close_authority.map_or(owner, |s| {
                        s.parse::<Pubkey>()
                            .unwrap_or_else(|err| panic!("Invalid close authority: {}", err))
                    });

                    let entry = accounts_by_token.entry(token).or_insert_with(HashMap::new);
                    entry.insert(
                        token_account,
                        (
                            token_amount,
                            ui_token_account.token_amount.decimals,
                            frozen,
                            close_authority,
                        ),
                    );
                }
            }
        }
    }

    let mut instructions = vec![];
    let mut lamports_needed = 0;

    for (token, accounts) in accounts_by_token.into_iter() {
        println_display(config, format!("Processing token: {}", token));
        let associated_token_account = get_associated_token_address(&owner, &token);
        let total_balance: u64 = accounts.values().map(|account| account.0).sum();

        if total_balance > 0 && !accounts.contains_key(&associated_token_account) {
            // Create the associated token account
            instructions.push(vec![create_associated_token_account(
                &config.signers[tx_info.fee_payer].pubkey(),
                &owner,
                &token,
            )]);
            lamports_needed += minimum_balance_for_rent_exemption;
        }

        for (address, (amount, decimals, frozen, close_authority)) in accounts {
            match (
                address == associated_token_account,
                close_empty_associated_accounts,
                total_balance > 0,
            ) {
                (true, _, true) => continue, // don't ever close associated token account with amount
                (true, false, _) => continue, // don't close associated token account if close_empty_associated_accounts isn't set
                (true, true, false) => println_display(
                    config,
                    format!("Closing Account {}", associated_token_account),
                ),
                _ => {}
            }

            if frozen {
                // leave frozen accounts alone
                continue;
            }

            let mut account_instructions = vec![];

            // Sanity check!
            // we shouldn't ever be here, but if we are here, abort!
            assert!(amount == 0 || address != associated_token_account);

            if amount > 0 {
                // Transfer the account balance into the associated token account
                account_instructions.push(transfer_checked(
                    &mundis_token_program::id(),
                    &address,
                    &token,
                    &associated_token_account,
                    &owner,
                    &multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
                    amount,
                    decimals,
                )?);
            }
            // Close the account if config.owner is able to
            if close_authority == owner {
                account_instructions.push(close_account(
                    &mundis_token_program::id(),
                    &address,
                    &owner,
                    &owner,
                    &multisigner_pubkeys.iter().collect::<Vec<_>>().as_slice(),
                )?);
            }

            if !account_instructions.is_empty() {
                instructions.push(account_instructions);
            }
        }
    }

    let mut result = String::from("");
    for tx_instructions in instructions {
        let tx_return = handle_tx(
            rpc_client,
            config,
            lamports_needed,
            tx_instructions,
            tx_info
        )?;
        result += &match tx_return {
            TransactionReturnData::CliSignature(signature) => {
                config.output_format.formatted_string(&signature)
            }
            TransactionReturnData::CliSignOnlyData(sign_only_data) => {
                config.output_format.formatted_string(&sign_only_data)
            }
        };
        result += "\n";
    }
    Ok(result)
}

pub fn parse_token_sync_native_command(
    matches: &ArgMatches<'_>,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Result<CliCommandInfo, CliError> {
    let mut bulk_signers: Vec<Option<Box<dyn Signer>>> = vec![];
    let (fee_payer_pubkey, nonce_account, nonce_authority_pubkey, _) =
        add_default_signers(matches, wallet_manager, &mut bulk_signers)?;

    let address = associated_token_address_for_token_or_override(
        matches,
        "address",
        default_signer,
        wallet_manager,
        Some(native_mint::id()),
    );

    let signer_info = default_signer.generate_unique_signers(
        bulk_signers,
        matches,
        wallet_manager,
    )?;

    Ok(CliCommandInfo {
        command: CliCommand::SyncTokenAccount {
            native_account_address: address,
            tx_info: create_tx_info(matches, &signer_info, fee_payer_pubkey, nonce_account, nonce_authority_pubkey),
        },
        signers: signer_info.signers,
    })
}

pub fn process_token_sync_native_command(
    rpc_client: &RpcClient,
    config: &CliConfig,
    native_account_address: Pubkey,
    tx_info: &TxInfo,
) -> ProcessResult {
    let tx_return = handle_tx(
        rpc_client,
        config,
        0,
        vec![sync_native(&mundis_token_program::id(), &native_account_address)?],
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
            lamports_to_mdis(required_balance),
            lamports_to_mdis(balance)
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

pub(crate) fn resolve_mint_info(
    rpc_client: &RpcClient,
    sign_only: bool,
    token_account: &Pubkey,
    mint_address: Option<Pubkey>,
    mint_decimals: Option<u8>,
) -> Result<(Pubkey, u8), Box<dyn std::error::Error>> {
    if !sign_only {
        let source_account = rpc_client
            .get_token_account(token_account)?
            .ok_or_else(|| format!("Could not find token account {}", token_account))?;
        let source_mint = Pubkey::from_str(&source_account.mint)?;
        if let Some(mint) = mint_address {
            if source_mint != mint {
                return Err(format!(
                    "Source {:?} does not contain {:?} tokens",
                    token_account, mint
                )
                    .into());
            }
        }
        Ok((source_mint, source_account.token_amount.decimals))
    } else {
        Ok((
            mint_address.unwrap_or_default(),
            mint_decimals.unwrap_or_default(),
        ))
    }
}

// Check if an explicit token account address was provided, otherwise
// return the associated token address for the default address.
pub(crate) fn associated_token_address_or_override(
    arg_matches: &ArgMatches,
    override_name: &str,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
) -> Pubkey {
    let token = pubkey_of_signer(arg_matches, "token", wallet_manager).unwrap();
    associated_token_address_for_token_or_override(
        arg_matches,
        override_name,
        default_signer,
        wallet_manager,
        token,
    )
}

// Check if an explicit token account address was provided, otherwise
// return the associated token address for the default address.
pub(crate) fn associated_token_address_for_token_or_override(
    arg_matches: &ArgMatches,
    override_name: &str,
    default_signer: &DefaultSigner,
    wallet_manager: &mut Option<Arc<RemoteWalletManager>>,
    token: Option<Pubkey>,
) -> Pubkey {
    if let Some(address) = pubkey_of_signer(arg_matches, override_name, wallet_manager).unwrap()
    {
        return address;
    }

    let token = token.unwrap();
    let owner = default_signer.signer_from_path(arg_matches, wallet_manager).unwrap().pubkey();
    get_associated_token_address(&owner, &token)
}

fn check_wallet_balance(
    rpc_client: &RpcClient,
    wallet: &Pubkey,
    required_balance: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let balance = rpc_client.get_balance(wallet)?;
    if balance < required_balance {
        Err(format!(
            "Wallet {}, has insufficient balance: {} required, {} available",
            wallet,
            lamports_to_mdis(required_balance),
            lamports_to_mdis(balance)
        )
            .into())
    } else {
        Ok(())
    }
}

fn validate_mint(rpc_client: &RpcClient, token: Pubkey) -> Result<(), Box<dyn std::error::Error>> {
    let mint = rpc_client.get_account(&token);
    if mint.is_err() || Mint::unpack(&mint.unwrap().data).is_err() {
        return Err(format!("Invalid mint account {:?}", token).into());
    }
    Ok(())
}

fn sort_and_parse_token_accounts(
    owner: &Pubkey,
    accounts: Vec<RpcKeyedAccount>,
) -> (BTreeMap<String, Vec<CliTokenAccount>>, Vec<UnsupportedAccount>, usize, bool) {
    let mut mint_accounts: BTreeMap<String, Vec<CliTokenAccount>> = BTreeMap::new();
    let mut unsupported_accounts = vec![];
    let mut max_len_balance = 0;
    let mut includes_aux = false;
    for keyed_account in accounts {
        let address = keyed_account.pubkey;

        if let UiAccountData::Json(parsed_account) = keyed_account.account.data {
            if parsed_account.program != "token" {
                unsupported_accounts.push(UnsupportedAccount {
                    address,
                    err: format!("Unsupported account program: {}", parsed_account.program),
                });
            } else {
                match serde_json::from_value(parsed_account.parsed) {
                    Ok(TokenAccountType::Account(ui_token_account)) => {
                        let mint = ui_token_account.mint.clone();
                        let is_associated = if let Ok(mint) = Pubkey::from_str(&mint) {
                            get_associated_token_address(owner, &mint).to_string() == address
                        } else {
                            includes_aux = true;
                            false
                        };
                        let len_balance = ui_token_account
                            .token_amount
                            .real_number_string_trimmed()
                            .len();
                        max_len_balance = max_len_balance.max(len_balance);
                        let parsed_account = CliTokenAccount {
                            address,
                            account: ui_token_account,
                            is_associated,
                        };
                        let entry = mint_accounts.entry(mint);
                        match entry {
                            Entry::Occupied(_) => {
                                entry.and_modify(|e| e.push(parsed_account));
                            }
                            Entry::Vacant(_) => {
                                entry.or_insert_with(|| vec![parsed_account]);
                            }
                        }
                    }
                    Ok(_) => unsupported_accounts.push(UnsupportedAccount {
                        address,
                        err: "Not a token account".to_string(),
                    }),
                    Err(err) => unsupported_accounts.push(UnsupportedAccount {
                        address,
                        err: format!("Account parse failure: {}", err),
                    }),
                }
            }
        } else {
            unsupported_accounts.push(UnsupportedAccount {
                address,
                err: "Unsupported account data format".to_string(),
            });
        }
    }
    for (_, array) in mint_accounts.iter_mut() {
        array.sort_by(|a, b| b.is_associated.cmp(&a.is_associated));
    }
    (
        mint_accounts,
        unsupported_accounts,
        max_len_balance,
        includes_aux,
    )
}

pub fn is_token_name_field(string: String) -> Result<(), String> {
    if string.len() > MAX_NAME_LENGTH {
        Err(format!(
            "token name field longer than {:?}-byte limit",
            MAX_NAME_LENGTH
        ))
    } else {
        Ok(())
    }
}

pub fn is_token_symbol_field(string: String) -> Result<(), String> {
    if string.len() > MAX_SYMBOL_LENGTH {
        Err(format!(
            "token symbol field longer than {:?}-byte limit",
            MAX_SYMBOL_LENGTH
        ))
    } else {
        Ok(())
    }
}