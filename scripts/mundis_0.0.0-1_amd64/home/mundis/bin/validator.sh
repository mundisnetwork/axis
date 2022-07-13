#!/usr/bin/env bash
set -ex
shopt -s nullglob

export RUST_BACKTRACE=1
export RUST_LOG=info

# Delete any zero-length snapshots that can cause validator startup to fail
find $LEDGER_DIR -name 'snapshot-*' -size 0 -print -exec rm {} \; || true

args=(
  --dynamic-port-range 8002-8015
  --gossip-port 8001
  --identity "$IDENTITY_KEYPAIR"
  --ledger "$LEDGER_DIR"
  --accounts "$ACCOUNTS_DIR"
  --limit-ledger-size
  --rpc-port 8899
  --expected-genesis-hash "$GENESIS_HASH"
  --wal-recovery-mode skip_any_corrupted_record
  --vote-account "$VOTE_ACCOUNT_KEYPAIR"
  --authorized-voter "$IDENTITY_KEYPAIR"
  --expected-shred-version "$SHRED_VERSION"
  --entrypoint entrypoint1.devnet.mundis.io:8001
  --entrypoint entrypoint2.devnet.mundis.io:8001
  --no-genesis-fetch
  --no-snapshot-fetch
  --log "$VALIDATOR_LOG_FILE"
  --no-wait-for-vote-to-start-leader
  --snapshot-interval-slots 200
)

exec mundis-validator "${args[@]}"
