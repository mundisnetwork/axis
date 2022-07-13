#!/usr/bin/env bash
set -ex
shopt -s nullglob

export RUST_BACKTRACE=1
export RUST_LOG=info

export MUNDIS_METRICS_CONFIG=host=http://metrics.devnet.mundis.io:8086,db=devnet,u=admin,p=Metaverse2022

# Delete any zero-length snapshots that can cause validator startup to fail
find $LEDGER_DIR -name 'snapshot-*' -size 0 -print -exec rm {} \; || true

identity_keypair=/home/mundis/api-identity.json

args=(
  --dynamic-port-range 8002-8015
  --gossip-port 8001
  --identity "$identity_keypair"
  --ledger "$LEDGER_DIR"
  --accounts "$ACCOUNTS_DIR"
  --limit-ledger-size
  --rpc-port 8899
  --expected-genesis-hash "$GENESIS_HASH"
  --wal-recovery-mode skip_any_corrupted_record
  --entrypoint entrypoint1.devnet.mundis.io:8001
  --entrypoint entrypoint2.devnet.mundis.io:8001
  --expected-shred-version 1583
  --log $API_LOG_FILE
  --enable-rpc-transaction-history
  --no-port-check
  --no-untrusted-rpc
  --skip-poh-verify
  --trusted-validator "$TRUSTED_VALIDATOR"
  --no-voting
  --rpc-faucet-address 127.0.0.1:9900
  --full-rpc-api
  --enable-cpi-and-log-storage
)

exec mundis-validator "${args[@]}"
