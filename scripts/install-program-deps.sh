#!/usr/bin/env bash

set -x

cargo --version
cargo install rustfilt || true
cargo install honggfuzz --version=0.5.52 --force || true
