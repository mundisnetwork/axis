#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

keygen() {
  declare cmd=$*

  mundis-keygen --version
  test -f /home/mundis/validator-identity.json ||
    (set -x; mundis-keygen $cmd --outfile /home/mundis/validator-identity.json)
  test -f /home/mundis/validator-vote-account.json ||
    (set -x; mundis-keygen $cmd --outfile /home/mundis/validator-vote-account.json)
  test -f /home/mundis/validator-stake-account.json ||
    (set -x; mundis-keygen $cmd --outfile /home/mundis/validator-stake-account.json)

  test -f /home/mundis/faucet.json ||
    (set -x; mundis-keygen $cmd --outfile /home/mundis/faucet.json)

  test -f /home/mundis/api-identity.json ||
    (set -x; mundis-keygen $cmd --outfile /home/mundis/api-identity.json)

  test -f /home/mundis/warehouse-identity.json ||
    (set -x; mundis-keygen $cmd --outfile /home/mundis/warehouse-identity.json)
}

case "$1" in
recover)
  keygen recover
  ;;
'')
  keygen new --no-passphrase
  ;;
*)
  echo "Error: unknown argument: -$1-"
  exit 1
  ;;
esac
