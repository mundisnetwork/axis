#!/bin/bash

set -ex

source /etc/default/mundis

GENESIS_HASH="$(RUST_LOG=none mundis-ledger-tool genesis-hash --ledger $LEDGER_DIR)"
SHRED_VERSION="$(RUST_LOG=none mundis-ledger-tool shred-version --ledger $LEDGER_DIR)"
BANK_HASH="$(RUST_LOG=none mundis-ledger-tool bank-hash --ledger $LEDGER_DIR)"

echo Genesis hash: $GENESIS_HASH
echo Shred version: $SHRED_VERSION
echo Bank hash: $BANK_HASH
