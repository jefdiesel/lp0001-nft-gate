#!/bin/bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# T12: an update of the gate's header is refused. The deploying wallet holds the header key.
# Pass: refused with "UpdateHeader target is immutable and cannot be updated"
# (program_loader core lib.rs:144-145). Note the wallet CLI's own help calls the flag "not
# protocol-enforced" (wallet cli/programs/program_loader.rs, UpdateHeader doc): this settles it.
. "$(dirname "$0")/env.sh"; . "$(dirname "$0")/lib.sh" T12
GATE=$(python3 -I -c "import json;print(json.load(open('$GATE_CONFIG'))['gate'])")
run header $CLI header --program $GATE
expect_text header "immutable true"
FIRST=$(awk '/^first_segment/{print $2}' $O/header.out)
run update $CLI wallet program-loader update-header --header "Public/$GATE" --first-segment $FIRST --payer "Public/$(acct pay)"
expect_refused update "immutable"
run header-after $CLI header --program $GATE
expect_text header-after "immutable true"
finish_test
