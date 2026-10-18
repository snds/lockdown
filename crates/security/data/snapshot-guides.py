#!/usr/bin/env python3
"""Rebuild `site-data/guides.json`, the two-step and passkey set-up guides.

Usage:
  python3 -I snapshot-guides.py <checkout of 2factorauth/twofactorauth> \
      <checkout of 2factorauth/passkeys> > site-data/guides.json

Two community directories feed the guides the security center shows for a
site that offers two-step sign-in you haven't added
(docs/plan/23-bridges.md §3):

- 2fa.directory (`2factorauth/twofactorauth`, MIT, see
  `2fa-directory.LICENSE`): the site's own notes on setting it up (a phone
  number needed first, one method at a time), its recovery page, and the
  names of its own apps or devices. The methods and the set-up page are
  already in `2fa-directory.json`; they are not repeated here.
- The Passkeys Directory (`2factorauth/passkeys`, CC BY 4.0, see
  `passkeys-directory.LICENSE`): whether the site takes a passkey to sign in
  (`passwordless`) or as a second step (`mfa`), and its page on it.

Only sites with something to add are kept. The same withheld names as the
2FA snapshot are left out (this repository's content rule). The output is
signed into `site-data.signed.cbor` by `cargo run -p lockdown-security
--example sign-site-data`.
"""

import glob
import hashlib
import json
import os
import subprocess
import sys

# The same withheld entries as snapshot-2fa-directory.py, by the SHA-256 of
# the entry's name, plus one the Passkeys Directory adds.
WITHHELD = {
    "a01e4f96a000a6943d2e9c815a5409665f275469bb885868fa0674d4fbcf8206",
    "4128ea9249ac4b045b2ad2dc6d728a56bf2794a9ee4649a8bf5949eef0a7596d",
    "f6ece80b88d6dfbd2f65dad5ccde8e5ec2a0d4dbfadc2fe87bdf58642f17e446",
    "421a7e6e57b56283cb44435c81a01acb86e19aaab5ff48409b96045aa724150e",
    # One more from the Passkeys Directory, under the same rule.
    "3e46fc8855ee48f2a4e8ea2e44d614bf95c5d69bf535443079ee773c72caf194",
}


def head(repo):
    return subprocess.run(
        ["git", "-C", repo, "log", "-1", "--format=%H %cs"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.split()


def https(value):
    return value if isinstance(value, str) and value.startswith("https://") else None


def names(value):
    if isinstance(value, list):
        return [v for v in value if isinstance(v, str) and 0 < len(v) <= 80]
    return []


def read(repo):
    for path in sorted(glob.glob(os.path.join(repo, "entries", "*", "*.json"))):
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        domain = os.path.basename(path)[: -len(".json")].lower()
        for name, e in data.items():
            if hashlib.sha256(name.encode("utf-8")).hexdigest() in WITHHELD:
                continue
            yield domain, name, e


def main(tfa_repo, pk_repo):
    guides = {}

    def entry(domain, name, e):
        domains = [e.get("domain", domain).lower()] + [
            d.lower() for d in e.get("additional-domains", []) if isinstance(d, str)
        ]
        g = guides.setdefault(domains[0], {"n": name, "d": domains})
        for d in domains:
            if d not in g["d"]:
                g["d"].append(d)
        return g

    for domain, name, e in read(tfa_repo):
        if not e.get("tfa"):
            continue
        notes = e.get("notes")
        recovery = https(e.get("recovery"))
        apps = names(e.get("custom-software"))
        devices = names(e.get("custom-hardware"))
        if not (notes or recovery or apps or devices):
            continue
        g = entry(domain, name, e)
        if isinstance(notes, str) and notes.strip():
            g["notes"] = " ".join(notes.split())[:400]
        if recovery:
            g["recovery"] = recovery
        if apps:
            g["apps"] = apps
        if devices:
            g["devices"] = devices
        g.setdefault("src", []).append("2fa")

    for domain, name, e in read(pk_repo):
        kinds = [k for k in ("passwordless", "mfa") if k in e]
        if not kinds:
            continue
        g = entry(domain, name, e)
        g["passkey"] = "+".join(kinds)
        doc = https(e.get("documentation"))
        if doc:
            g["passkey_doc"] = doc
        g.setdefault("src", []).append("passkeys")

    tfa_head, pk_head = head(tfa_repo), head(pk_repo)
    out = {
        "sources": [
            {
                "id": "2fa",
                "name": "2fa.directory",
                "url": "https://github.com/2factorauth/twofactorauth",
                "licence": "MIT",
                "commit": tfa_head[0],
                "date": tfa_head[1],
            },
            {
                "id": "passkeys",
                "name": "Passkeys Directory",
                "url": "https://github.com/2factorauth/passkeys",
                "licence": "CC BY 4.0",
                "commit": pk_head[0],
                "date": pk_head[1],
            },
        ],
        "guides": sorted(guides.values(), key=lambda g: (g["d"][0], g["n"])),
    }
    json.dump(out, sys.stdout, ensure_ascii=False, indent=1, sort_keys=True)
    sys.stdout.write("\n")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
