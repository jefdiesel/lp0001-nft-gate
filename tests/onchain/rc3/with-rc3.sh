#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Run one on-chain script against our public rc3 chain:  rc3/with-rc3.sh ../03-deploy-gate.sh
set -u
. "$(cd "$(dirname "$0")" && pwd)/env-rc3.sh"
exec "$@"
