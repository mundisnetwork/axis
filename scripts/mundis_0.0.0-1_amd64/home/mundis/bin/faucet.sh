#!/bin/bash -ex

exec mundis-faucet \
  --keypair /home/mundis/faucet.json \
  --per-request-cap 10 \
  --per-time-cap 50 \
  --slice 10
