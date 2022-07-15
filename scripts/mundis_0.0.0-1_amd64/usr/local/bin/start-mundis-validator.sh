#!/usr/bin/env bash
set -ex
shopt -s nullglob

export RUST_BACKTRACE=1
export RUST_LOG=info

source /etc/default/mundis

export MUNDIS_METRICS_CONFIG=host=http://metrics.devnet.mundis.io:8086,db=devnet,u=admin,p=Metaverse2022

# Delete any zero-length snapshots that can cause validator startup to fail
find $LEDGER_DIR -name 'snapshot-*' -size 0 -print -exec rm {} \; || true

args=(
  --dynamic-port-range 8002-8015
  --gossip-port 8001
  --identity "$VALIDATOR_IDENTITY_KEYPAIR"
  --ledger "$LEDGER_DIR"
  --accounts "$ACCOUNTS_DIR"
  --limit-ledger-size
  --rpc-port 8899
  --expected-genesis-hash "$GENESIS_HASH"
  --wal-recovery-mode skip_any_corrupted_record
  --vote-account "$VOTE_ACCOUNT_KEYPAIR"
  --authorized-voter "$VALIDATOR_IDENTITY_KEYPAIR"
  --expected-shred-version "$SHRED_VERSION"
  --entrypoint entrypoint1.devnet.mundis.io:8001
  --entrypoint entrypoint2.devnet.mundis.io:8001
  --log "$VALIDATOR_LOG_FILE"
  --full-rpc-api
  --enable-rpc-transaction-history
  --snapshot-interval-slots 200
  --enable-cpi-and-log-storage
  --known-validator "$TRUSTED_VALIDATOR"
)

exec mundis-validator "${args[@]}"
