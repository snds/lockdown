#!/usr/bin/env python3
"""Rebuild `2fa-directory.json`, the bundled snapshot of 2fa.directory.

Usage: python3 -I snapshot-2fa-directory.py <checkout of 2factorauth/twofactorauth> > 2fa-directory.json

The source is the community's data repository (MIT licence, see
`2fa-directory.LICENSE`). Only sites that offer two-step sign-in are kept,
and of each only what the missing-2FA report reads: the name, the domains,
the methods and the set-up page. The commit and its date are recorded so a
shell can say how old the snapshot is.
"""

import glob
import hashlib
import json
import os
import subprocess
import sys

METHODS = {"totp", "sms", "email", "call", "u2f", "custom-software", "custom-hardware"}

# Entries left out of the snapshot because their names fall under this
# repository's content rule (no AI product or employer names in repository
# content). They are named here only by the SHA-256 of the entry name; their
# sites simply get no missing-2FA finding.
WITHHELD = {
    "a01e4f96a000a6943d2e9c815a5409665f275469bb885868fa0674d4fbcf8206",
    "4128ea9249ac4b045b2ad2dc6d728a56bf2794a9ee4649a8bf5949eef0a7596d",
    "f6ece80b88d6dfbd2f65dad5ccde8e5ec2a0d4dbfadc2fe87bdf58642f17e446",
    "421a7e6e57b56283cb44435c81a01acb86e19aaab5ff48409b96045aa724150e",
}


def main(repo: str) -> None:
    commit = subprocess.run(
        ["git", "-C", repo, "log", "-1", "--format=%H %cs"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.split()
    entries = []
    for path in sorted(glob.glob(os.path.join(repo, "entries", "*", "*.json"))):
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        for name, e in data.items():
            if hashlib.sha256(name.encode("utf-8")).hexdigest() in WITHHELD:
                continue
            methods = sorted(m for m in e.get("tfa", []) if m in METHODS)
            if not methods:
                continue
            domains = [e["domain"].lower()] + [d.lower() for d in e.get("additional-domains", [])]
            entry = {"n": name, "d": domains, "m": methods}
            doc = e.get("documentation")
            if isinstance(doc, str) and doc.startswith("https://"):
                entry["doc"] = doc
            entries.append(entry)
    entries.sort(key=lambda x: (x["d"][0], x["n"]))
    out = {
        "v": 1,
        "source": "2fa.directory, https://github.com/2factorauth/twofactorauth",
        "licence": "MIT",
        "commit": commit[0],
        "date": commit[1],
        "entries": entries,
    }
    json.dump(out, sys.stdout, ensure_ascii=False, separators=(",", ":"))
    sys.stdout.write("\n")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
