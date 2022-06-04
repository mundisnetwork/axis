#!/usr/bin/env bash

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd -P)

if [[ -z "${GITHUB_TAG}" ]]; then
  echo "GITHUB_TAG is not defined"
  exit 1
fi

S3_DEST="s3://release.mundis.io"
BINARIES=('mundis' 'mundis-faucet' 'mundis-genesis' 'mundis-gossip' 'mundis-keygen' 'mundis-ledger-tool' 'mundis-validator')


for binary in "${BINARIES[@]}"
do
	DEST_LOCATION="$S3_DEST/$GITHUB_TAG/$binary"
	echo "Uploading '$binary' to $DEST_LOCATION"
	aws s3 cp "$SCRIPT_DIR/../target/release/$binary" $DEST_LOCATION
done

aws cloudfront create-invalidation --distribution-id E15DK7CRQHN9TM --paths "/*"
exit 0
