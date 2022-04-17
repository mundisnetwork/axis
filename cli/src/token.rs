use std::str::FromStr;
use clap::{App, AppSettings, Arg, SubCommand};
use mundis_clap_utils::{ArgConstant, offline};
use mundis_clap_utils::fee_payer::fee_payer_arg;
use mundis_clap_utils::input_validators::{is_amount, is_amount_or_all, is_parsable, is_valid_pubkey, is_valid_signer};
use mundis_clap_utils::memo::memo_arg;
use mundis_clap_utils::nonce::NonceArgs;
use mundis_clap_utils::offline::{BLOCKHASH_ARG, OfflineArgs, SIGN_ONLY_ARG};
use mundis_token_program::native_mint;
use mundis_token_program::token_instruction::{MAX_SIGNERS, MIN_SIGNERS};

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
                        .offline_args_config(&SignOnlyNeedsMintDecimals{}),
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
                        .offline_args_config(&SignOnlyNeedsFullMintSpec{}),
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
                        .offline_args_config(&SignOnlyNeedsMintDecimals{}),
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
                        .offline_args_config(&SignOnlyNeedsMintAddress{}),
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
                        .offline_args_config(&SignOnlyNeedsMintAddress{}),
                )
                .subcommand(
                    SubCommand::with_name("wrap")
                        .about("Wrap native SOL in a SOL token account")
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
                        .offline_args_config(&SignOnlyNeedsFullMintSpec{}),
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
                        .offline_args_config(&SignOnlyNeedsDelegateAddress{}),
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
                        .about("Query details of an SPL Token account by address")
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
                        .about("Query details about and SPL Token multisig account by address")
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
                    SubCommand::with_name("gc")
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
                        .about("Sync a native SOL token account to its underlying lamports")
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