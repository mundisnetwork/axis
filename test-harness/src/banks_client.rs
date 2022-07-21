//! A client for the ledger state, from the perspective of an arbitrary validator.
//!
//! Use start_tcp_client() to create a client and then import BanksClientExt to
//! access its methods. Additional "*_with_context" methods are also available,
//! but they are undocumented, may change over time, and are generally more
//! cumbersome to use.

pub use crate::error::BanksClientError;
pub use crate::{BanksClient as TarpcClient, TransactionStatus};
use {
    borsh::BorshDeserialize,
    futures::{future::join_all, Future, FutureExt, TryFutureExt},
    crate::{BanksRequest, BanksResponse, BanksTransactionResultWithSimulation},
    mundis_program::{
        clock::Slot, hash::Hash, program_pack::Pack, pubkey::Pubkey, sysvar::Sysvar,
    },
    mundis_sdk::{
        account::{from_account, Account},
        commitment_config::CommitmentLevel,
        message::Message,
        signature::Signature,
        transaction::{self, Transaction},
        transport,
    },
    std::io,
    tarpc::{
        client::{self, NewClient, RequestDispatch},
        context::{self, Context},
        ClientMessage, Response, Transport,
    },
    tokio::time::Duration,
};
use mundis_program::fee_calculator::FeeCalculator;
use mundis_program::rent::Rent;

#[derive(Clone)]
pub struct BanksClient {
    inner: TarpcClient,
}

impl BanksClient {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<C>(
        config: client::Config,
        transport: C,
    ) -> NewClient<TarpcClient, RequestDispatch<BanksRequest, BanksResponse, C>>
        where
            C: Transport<ClientMessage<BanksRequest>, Response<BanksResponse>>,
    {
        TarpcClient::new(config, transport)
    }

    pub fn send_transaction_with_context(
        &mut self,
        ctx: Context,
        transaction: Transaction,
    ) -> impl Future<Output = io::Result<()>> + '_ {
        self.inner
            .send_transaction_with_context(ctx, transaction)
            .map_err(BanksClientError::from) // Remove this when return Err type updated to BanksClientError
            .map_err(Into::into)
    }

    pub fn get_fees_with_commitment_and_context(
        &mut self,
        ctx: Context,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = io::Result<(FeeCalculator, Hash, u64)>> + '_ {
        #[allow(deprecated)]
        self.inner
            .get_fees_with_commitment_and_context(ctx, commitment)
            .map_err(BanksClientError::from) // Remove this when return Err type updated to BanksClientError
            .map_err(Into::into)
    }

    pub fn get_transaction_status_with_context(
        &mut self,
        ctx: Context,
        signature: Signature,
    ) -> impl Future<Output = io::Result<Option<TransactionStatus>>> + '_ {
        self.inner
            .get_transaction_status_with_context(ctx, signature)
            .map_err(BanksClientError::from) // Remove this when return Err type updated to BanksClientError
            .map_err(Into::into)
    }

    pub fn get_slot_with_context(
        &mut self,
        ctx: Context,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = io::Result<Slot>> + '_ {
        self.inner
            .get_slot_with_context(ctx, commitment)
            .map_err(BanksClientError::from) // Remove this when return Err type updated to BanksClientError
            .map_err(Into::into)
    }

    pub fn get_block_height_with_context(
        &mut self,
        ctx: Context,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = io::Result<Slot>> + '_ {
        self.inner
            .get_block_height_with_context(ctx, commitment)
            .map_err(BanksClientError::from) // Remove this when return Err type updated to BanksClientError
            .map_err(Into::into)
    }

    pub fn process_transaction_with_commitment_and_context(
        &mut self,
        ctx: Context,
        transaction: Transaction,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = io::Result<Option<transaction::Result<()>>>> + '_ {
        self.inner
            .process_transaction_with_commitment_and_context(ctx, transaction, commitment)
            .map_err(BanksClientError::from) // Remove this when return Err type updated to BanksClientError
            .map_err(Into::into)
    }

    pub fn process_transaction_with_preflight_and_commitment_and_context(
        &mut self,
        ctx: Context,
        transaction: Transaction,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = Result<BanksTransactionResultWithSimulation, BanksClientError>> + '_
    {
        self.inner
            .process_transaction_with_preflight_and_commitment_and_context(
                ctx,
                transaction,
                commitment,
            )
            .map_err(Into::into)
    }

    pub fn get_account_with_commitment_and_context(
        &mut self,
        ctx: Context,
        address: Pubkey,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = io::Result<Option<Account>>> + '_ {
        self.inner
            .get_account_with_commitment_and_context(ctx, address, commitment)
            .map_err(BanksClientError::from) // Remove this when return Err type updated to BanksClientError
            .map_err(Into::into)
    }

    /// Send a transaction and return immediately. The server will resend the
    /// transaction until either it is accepted by the cluster or the transaction's
    /// blockhash expires.
    pub fn send_transaction(
        &mut self,
        transaction: Transaction,
    ) -> impl Future<Output = io::Result<()>> + '_ {
        self.send_transaction_with_context(context::current(), transaction)
    }

    /// Return the cluster Sysvar
    pub fn get_sysvar<T: Sysvar>(&mut self) -> impl Future<Output = io::Result<T>> + '_ {
        self.get_account(T::id()).map(|result| {
            let sysvar = result?
                .ok_or(BanksClientError::ClientError("Sysvar not present"))
                .map_err(io::Error::from)?; // Remove this map when return Err type updated to BanksClientError
            from_account::<T, _>(&sysvar)
                .ok_or(BanksClientError::ClientError(
                    "Failed to deserialize sysvar",
                ))
                .map_err(Into::into) // Remove this when return Err type updated to BanksClientError
        })
    }

    pub fn get_fees(
        &mut self,
    ) -> impl Future<Output = io::Result<(FeeCalculator, Hash, u64)>> + '_ {
        #[allow(deprecated)]
        self.get_fees_with_commitment_and_context(context::current(), CommitmentLevel::default())
    }

    /// Return the cluster rent
    pub fn get_rent(&mut self) -> impl Future<Output = io::Result<Rent>> + '_ {
        self.get_sysvar::<Rent>()
    }

    /// Send a transaction and return after the transaction has been rejected or
    /// reached the given level of commitment.
    pub fn process_transaction_with_commitment(
        &mut self,
        transaction: Transaction,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = transport::Result<()>> + '_ {
        let mut ctx = context::current();
        ctx.deadline += Duration::from_secs(50);
        self.process_transaction_with_commitment_and_context(ctx, transaction, commitment)
            .map(|result| match result? {
                None => Err(BanksClientError::ClientError(
                    "invalid blockhash or fee-payer",
                )),
                Some(transaction_result) => Ok(transaction_result?),
            })
            .map_err(Into::into) // Remove this when return Err type updated to BanksClientError
    }

    /// Send a transaction and return any preflight (sanitization or simulation) errors, or return
    /// after the transaction has been rejected or reached the given level of commitment.
    pub fn process_transaction_with_preflight_and_commitment(
        &mut self,
        transaction: Transaction,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = Result<(), BanksClientError>> + '_ {
        let mut ctx = context::current();
        ctx.deadline += Duration::from_secs(50);
        self.process_transaction_with_preflight_and_commitment_and_context(
            ctx,
            transaction,
            commitment,
        )
            .map(|result| match result? {
                BanksTransactionResultWithSimulation {
                    result: None,
                    simulation_details: _,
                } => Err(BanksClientError::ClientError(
                    "invalid blockhash or fee-payer",
                )),
                BanksTransactionResultWithSimulation {
                    result: Some(Err(err)),
                    simulation_details: Some(simulation_details),
                } => Err(BanksClientError::SimulationError {
                    err,
                    logs: simulation_details.logs,
                    units_consumed: simulation_details.units_consumed,
                }),
                BanksTransactionResultWithSimulation {
                    result: Some(result),
                    simulation_details: _,
                } => result.map_err(Into::into),
            })
    }

    /// Send a transaction and return any preflight (sanitization or simulation) errors, or return
    /// after the transaction has been finalized or rejected.
    pub fn process_transaction_with_preflight(
        &mut self,
        transaction: Transaction,
    ) -> impl Future<Output = Result<(), BanksClientError>> + '_ {
        self.process_transaction_with_preflight_and_commitment(
            transaction,
            CommitmentLevel::default(),
        )
    }

    /// Send a transaction and return until the transaction has been finalized or rejected.
    pub fn process_transaction(
        &mut self,
        transaction: Transaction,
    ) -> impl Future<Output = transport::Result<()>> + '_ {
        self.process_transaction_with_commitment(transaction, CommitmentLevel::default())
    }

    pub async fn process_transactions_with_commitment(
        &mut self,
        transactions: Vec<Transaction>,
        commitment: CommitmentLevel,
    ) -> transport::Result<()> {
        let mut clients: Vec<_> = transactions.iter().map(|_| self.clone()).collect();
        let futures = clients
            .iter_mut()
            .zip(transactions)
            .map(|(client, transaction)| {
                client.process_transaction_with_commitment(transaction, commitment)
            });
        let statuses = join_all(futures).await;
        statuses.into_iter().collect() // Convert Vec<Result<_, _>> to Result<Vec<_>>
    }

    /// Send transactions and return until the transaction has been finalized or rejected.
    pub fn process_transactions(
        &mut self,
        transactions: Vec<Transaction>,
    ) -> impl Future<Output = transport::Result<()>> + '_ {
        self.process_transactions_with_commitment(transactions, CommitmentLevel::default())
    }

    /// Return the most recent rooted slot. All transactions at or below this slot
    /// are said to be finalized. The cluster will not fork to a higher slot.
    pub fn get_root_slot(&mut self) -> impl Future<Output = io::Result<Slot>> + '_ {
        self.get_slot_with_context(context::current(), CommitmentLevel::default())
    }

    /// Return the most recent rooted block height. All transactions at or below this height
    /// are said to be finalized. The cluster will not fork to a higher block height.
    pub fn get_root_block_height(&mut self) -> impl Future<Output = io::Result<Slot>> + '_ {
        self.get_block_height_with_context(context::current(), CommitmentLevel::default())
    }

    /// Return the account at the given address at the slot corresponding to the given
    /// commitment level. If the account is not found, None is returned.
    pub fn get_account_with_commitment(
        &mut self,
        address: Pubkey,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = io::Result<Option<Account>>> + '_ {
        self.get_account_with_commitment_and_context(context::current(), address, commitment)
    }

    /// Return the account at the given address at the time of the most recent root slot.
    /// If the account is not found, None is returned.
    pub fn get_account(
        &mut self,
        address: Pubkey,
    ) -> impl Future<Output = io::Result<Option<Account>>> + '_ {
        self.get_account_with_commitment(address, CommitmentLevel::default())
    }

    /// Return the unpacked account data at the given address
    /// If the account is not found, an error is returned
    pub fn get_packed_account_data<T: Pack>(
        &mut self,
        address: Pubkey,
    ) -> impl Future<Output = io::Result<T>> + '_ {
        self.get_account(address).map(|result| {
            let account = result?
                .ok_or(BanksClientError::ClientError("Account not found"))
                .map_err(io::Error::from)?; // Remove this map when return Err type updated to BanksClientError
            T::unpack_from_slice(&account.data)
                .map_err(|_| BanksClientError::ClientError("Failed to deserialize account"))
                .map_err(Into::into) // Remove this when return Err type updated to BanksClientError
        })
    }

    /// Return the unpacked account data at the given address
    /// If the account is not found, an error is returned
    pub fn get_account_data_with_borsh<T: BorshDeserialize>(
        &mut self,
        address: Pubkey,
    ) -> impl Future<Output = io::Result<T>> + '_ {
        self.get_account(address).map(|result| {
            let account = result?.ok_or(BanksClientError::ClientError("Account not found"))?;
            T::try_from_slice(&account.data).map_err(Into::into)
        })
    }

    /// Return the balance in lamports of an account at the given address at the slot
    /// corresponding to the given commitment level.
    pub fn get_balance_with_commitment(
        &mut self,
        address: Pubkey,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = io::Result<u64>> + '_ {
        self.get_account_with_commitment_and_context(context::current(), address, commitment)
            .map(|result| Ok(result?.map(|x| x.lamports).unwrap_or(0)))
    }

    /// Return the balance in lamports of an account at the given address at the time
    /// of the most recent root slot.
    pub fn get_balance(&mut self, address: Pubkey) -> impl Future<Output = io::Result<u64>> + '_ {
        self.get_balance_with_commitment(address, CommitmentLevel::default())
    }

    /// Return the status of a transaction with a signature matching the transaction's first
    /// signature. Return None if the transaction is not found, which may be because the
    /// blockhash was expired or the fee-paying account had insufficient funds to pay the
    /// transaction fee. Note that servers rarely store the full transaction history. This
    /// method may return None if the transaction status has been discarded.
    pub fn get_transaction_status(
        &mut self,
        signature: Signature,
    ) -> impl Future<Output = io::Result<Option<TransactionStatus>>> + '_ {
        self.get_transaction_status_with_context(context::current(), signature)
    }

    /// Same as get_transaction_status, but for multiple transactions.
    pub async fn get_transaction_statuses(
        &mut self,
        signatures: Vec<Signature>,
    ) -> io::Result<Vec<Option<TransactionStatus>>> {
        // tarpc futures oddly hold a mutable reference back to the client so clone the client upfront
        let mut clients_and_signatures: Vec<_> = signatures
            .into_iter()
            .map(|signature| (self.clone(), signature))
            .collect();

        let futs = clients_and_signatures
            .iter_mut()
            .map(|(client, signature)| client.get_transaction_status(*signature));

        let statuses = join_all(futs).await;

        // Convert Vec<Result<_, _>> to Result<Vec<_>>
        statuses.into_iter().collect()
    }

    pub fn get_latest_blockhash(&mut self) -> impl Future<Output = io::Result<Hash>> + '_ {
        self.get_latest_blockhash_with_commitment(CommitmentLevel::default())
            .map(|result| {
                result?
                    .map(|x| x.0)
                    .ok_or(BanksClientError::ClientError("valid blockhash not found"))
                    .map_err(Into::into)
            })
    }

    pub fn get_latest_blockhash_with_commitment(
        &mut self,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = io::Result<Option<(Hash, u64)>>> + '_ {
        self.get_latest_blockhash_with_commitment_and_context(context::current(), commitment)
    }

    pub fn get_latest_blockhash_with_commitment_and_context(
        &mut self,
        ctx: Context,
        commitment: CommitmentLevel,
    ) -> impl Future<Output = io::Result<Option<(Hash, u64)>>> + '_ {
        self.inner
            .get_latest_blockhash_with_commitment_and_context(ctx, commitment)
            .map_err(BanksClientError::from) // Remove this when return Err type updated to BanksClientError
            .map_err(Into::into)
    }

    pub fn get_fee_for_message_with_commitment_and_context(
        &mut self,
        ctx: Context,
        commitment: CommitmentLevel,
        message: Message,
    ) -> impl Future<Output = io::Result<Option<u64>>> + '_ {
        self.inner
            .get_fee_for_message_with_commitment_and_context(ctx, commitment, message)
            .map_err(BanksClientError::from) // Remove this when return Err type updated to BanksClientError
            .map_err(Into::into)
    }
}

pub async fn start_client<C>(transport: C) -> io::Result<BanksClient>
    where
        C: Transport<ClientMessage<BanksRequest>, Response<BanksResponse>> + Send + 'static,
{
    Ok(BanksClient {
        inner: TarpcClient::new(client::Config::default(), transport).spawn(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};
    use std::time::Duration;
    use tarpc::{client, transport};
    use tokio::runtime::Runtime;
    use tokio::time::sleep;
    use mundis_runtime::bank::Bank;
    use mundis_runtime::bank_forks::BankForks;
    use mundis_runtime::commitment::BlockCommitmentCache;
    use mundis_runtime::genesis_utils::create_genesis_config;
    use mundis_sdk::commitment_config::CommitmentLevel;
    use mundis_sdk::message::Message;
    use mundis_sdk::signer::Signer;
    use mundis_sdk::system_instruction;
    use mundis_sdk::transaction::Transaction;
    use crate::banks_client::start_client;
    use crate::banks_server::start_local_server;
    use crate::error::BanksClientError;
    use super::BanksClient;

    #[test]
    fn test_banks_client_new() {
        let (client_transport, _server_transport) = transport::channel::unbounded();
        BanksClient::new(client::Config::default(), client_transport);
    }

    #[test]
    fn test_banks_server_transfer_via_server() -> Result<(), BanksClientError> {
        // This test shows the preferred way to interact with BanksServer.
        // It creates a runtime explicitly (no globals via tokio macros) and calls
        // `runtime.block_on()` just once, to run all the async code.

        let genesis = create_genesis_config(10);
        let bank = Bank::new_for_tests(&genesis.genesis_config);
        let slot = bank.slot();
        let block_commitment_cache = Arc::new(RwLock::new(
            BlockCommitmentCache::new_for_tests_with_slots(slot, slot),
        ));
        let bank_forks = Arc::new(RwLock::new(BankForks::new(bank)));

        let bob_pubkey = mundis_sdk::pubkey::new_rand();
        let mint_pubkey = genesis.mint_keypair.pubkey();
        let instruction = system_instruction::transfer(&mint_pubkey, &bob_pubkey, 1);
        let message = Message::new(&[instruction], Some(&mint_pubkey));

        Runtime::new()?.block_on(async {
            let client_transport =
                start_local_server(bank_forks, block_commitment_cache, Duration::from_millis(1))
                    .await;
            let mut banks_client = start_client(client_transport).await?;

            let recent_blockhash = banks_client.get_latest_blockhash().await?;
            let transaction = Transaction::new(&[&genesis.mint_keypair], message, recent_blockhash);
            banks_client.process_transaction(transaction).await.unwrap();
            assert_eq!(banks_client.get_balance(bob_pubkey).await?, 1);
            Ok(())
        })
    }

    #[test]
    fn test_banks_server_transfer_via_client() -> Result<(), BanksClientError> {
        // The caller may not want to hold the connection open until the transaction
        // is processed (or blockhash expires). In this test, we verify the
        // server-side functionality is available to the client.

        let genesis = create_genesis_config(10);
        let bank = Bank::new_for_tests(&genesis.genesis_config);
        let slot = bank.slot();
        let block_commitment_cache = Arc::new(RwLock::new(
            BlockCommitmentCache::new_for_tests_with_slots(slot, slot),
        ));
        let bank_forks = Arc::new(RwLock::new(BankForks::new(bank)));

        let mint_pubkey = &genesis.mint_keypair.pubkey();
        let bob_pubkey = mundis_sdk::pubkey::new_rand();
        let instruction = system_instruction::transfer(mint_pubkey, &bob_pubkey, 1);
        let message = Message::new(&[instruction], Some(mint_pubkey));

        Runtime::new()?.block_on(async {
            let client_transport =
                start_local_server(bank_forks, block_commitment_cache, Duration::from_millis(1))
                    .await;
            let mut banks_client = start_client(client_transport).await?;
            let (recent_blockhash, last_valid_block_height) = banks_client
                .get_latest_blockhash_with_commitment(CommitmentLevel::default())
                .await?
                .unwrap();
            let transaction = Transaction::new(&[&genesis.mint_keypair], message, recent_blockhash);
            let signature = transaction.signatures[0];
            banks_client.send_transaction(transaction).await?;

            let mut status = banks_client.get_transaction_status(signature).await?;

            while status.is_none() {
                let root_block_height = banks_client.get_root_block_height().await?;
                if root_block_height > last_valid_block_height {
                    break;
                }
                sleep(Duration::from_millis(100)).await;
                status = banks_client.get_transaction_status(signature).await?;
            }
            assert!(status.unwrap().err.is_none());
            assert_eq!(banks_client.get_balance(bob_pubkey).await?, 1);
            Ok(())
        })
    }
}