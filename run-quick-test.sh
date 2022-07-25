#!/bin/sh

cargo test --release --no-fail-fast --workspace \
  --exclude 'mundis-bloom' \
  --exclude 'mundis-clap-utils' \
  --exclude 'mundis-poh' \
  --exclude 'mundis-perf' \
  --exclude 'mundis-gossip' \
  --exclude 'mundis-genesis' \
  --exclude 'mundis-merkle-tree' \
  --exclude 'mundis-metrics' \
  --exclude 'mundis-ledger' \
  --exclude 'mundis-ledger-tool' \
  --exclude 'mundis-net-utils' \
  --exclude 'mundis-replica*' \
  --exclude 'mundis-storage*' \
  --exclude 'mundis-streamer' \
  --exclude 'mundis-rayon-threadlimit' \
  --exclude 'mundis-remote-wallet' \
