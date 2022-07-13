#!/usr/bin/env bash

set -ex

dpkg-deb --build --root-owner-group mundis_0.0.0-1_amd64
