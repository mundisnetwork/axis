#!/usr/bin/env bash

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd -P)

if [[ -z "${GITHUB_TAG}" ]]; then
  echo "GITHUB_TAG is not defined"
  exit 1
fi

S3_DEST="s3://release.mundis.io"
BINARIES=('mundis' 'mundis-faucet' 'mundis-genesis' 'mundis-gossip' 'mundis-keygen' 'mundis-ledger-tool' 'mundis-validator')

cp -f "$SCRIPT_DIR/installer" /tmp/installer
sed -i "s/MUNDIS_RELEASE=v0.0.0/MUNDIS_RELEASE=$GITHUB_TAG/g" /tmp/installer
aws s3 cp /tmp/installer "$S3_DEST/$GITHUB_TAG/installer"
rm -f /tmp/installer

for binary in "${BINARIES[@]}"
do
	DEST_LOCATION="$S3_DEST/$GITHUB_TAG/$binary"
	echo "Uploading '$binary' to $DEST_LOCATION"
	aws s3 cp "$SCRIPT_DIR/../target/release/$binary" $DEST_LOCATION
done

aws cloudfront create-invalidation --distribution-id E15DK7CRQHN9TM --paths "/*"
exit 0
