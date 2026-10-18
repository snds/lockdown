#!/usr/bin/env python3
"""Writes and checks the age interop samples (docs/plan/23-bridges.md §11).

Usage: python3 -I make-age-fixtures.py <crates/cxf/tests/fixtures>

Needs `pyrage` (Python bindings to rage, the age reference implementation
in Rust; independent of Lockdown's own age code). It

1. writes `age-reference.age` and `age-reference.armored.age`: the sample
   text sealed by rage to the fixed test key in `age-reference.key`, which
   Lockdown's test must open;
2. opens `age-from-lockdown.age` (written by Lockdown's own code, with
   `LOCKDOWN_WRITE_AGE=1 cargo test -p lockdown-cxf --test age_interop`)
   with rage, and fails unless it holds the sample text: Lockdown's files
   open in the reference implementation.

The key is a test key made for these files; it protects nothing else.
"""

import os
import sys

import pyrage
from pyrage import x25519

KEY = "AGE-SECRET-KEY-18MH3WW2HXQDTUPAZ9WG5SCYNDGNKT4HPZF722Z28V3RSUZVQ94ZQZ850ZT"


def sample():
    line = b"Lockdown age interop sample; every value made up.\n"
    return line * (70000 // len(line) + 1)


def main(out):
    identity = x25519.Identity.from_str(KEY)
    recipient = identity.to_public()
    with open(os.path.join(out, "age-reference.key"), "w") as f:
        f.write(KEY + "\n")
    with open(os.path.join(out, "age-reference.age"), "wb") as f:
        f.write(pyrage.encrypt(sample(), [recipient]))
    with open(os.path.join(out, "age-reference.armored.age"), "wb") as f:
        f.write(pyrage.encrypt(sample(), [recipient], armored=True))
    ours = os.path.join(out, "age-from-lockdown.age")
    if os.path.exists(ours):
        with open(ours, "rb") as f:
            plain = pyrage.decrypt(f.read(), [identity])
        if plain != sample():
            sys.exit("rage opened Lockdown's file but the text differs")
        print("rage opened Lockdown's age-from-lockdown.age")
    print("recipient:", recipient)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
