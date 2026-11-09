#!/usr/bin/env python3
"""Writes the updater manifest `latest.json` for the "main" channel
(docs/plan/60-auto-update.md §2), in the static form Tauri's updater reads.

    python3 packaging/update/write-manifest.py \\
        --version 0.1.0-main.20261107093000 --pub-date 2026-11-07T09:30:00Z \\
        --base-url https://github.com/snds/lockdown/releases/download/main-build \\
        --bundle darwin=dist/Lockdown_..._universal.app.tar.gz \\
        --bundle windows=dist/Lockdown_..._x64-setup.exe \\
        --bundle linux=dist/lockdown_..._amd64.deb \\
        [--server-image-digest sha256:<64 hex>] \\
        [--previous old-latest.json] --out dist/latest.json

Each bundle has its minisign signature beside it (`<file>.sig`, as
`tauri build` and `tauri signer sign` write it). Refused, with a reason:

- a version that isn't `<x.y.z>-main.<YYYYMMDDHHMMSS>`;
- a base URL that isn't https on github.com;
- a missing or empty signature, or one that isn't a minisign signature
  bound to this version (its trusted comment names `version:<version>`,
  which the app requires);
- with --previous, a version that isn't newer than the one published, so
  a late run never takes the channel backwards;
- a server image digest that isn't `sha256:` and 64 lower-case hex.

With --server-image-digest, the manifest also names the server image
published for this version (`"server_image": {"digest": …}`), so a server's
one-click Update can ask the host helper for exactly that build
(docs/plan/61-server-updates.md §6). The desktop app ignores the field.

The app reads the platform keys `darwin-aarch64`, `darwin-x86_64` (one
universal bundle), `windows-x86_64` (the NSIS installer) and
`linux-x86_64` (the deb, which the app only links to).
"""

import argparse
import base64
import json
import re
import sys
from pathlib import Path
from urllib.parse import quote, urlparse

VERSION = re.compile(r"^(\d+)\.(\d+)\.(\d+)-main\.(\d{14})$")
DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")

PLATFORMS = {
    "darwin": ["darwin-aarch64", "darwin-x86_64", "darwin-aarch64-app", "darwin-x86_64-app"],
    "windows": ["windows-x86_64", "windows-x86_64-nsis"],
    "linux": ["linux-x86_64", "linux-x86_64-deb"],
}


class Refused(Exception):
    pass


def version_key(version: str):
    m = VERSION.match(version)
    if not m:
        raise Refused(f"{version!r} isn't <x.y.z>-main.<YYYYMMDDHHMMSS>")
    return tuple(int(g) for g in m.groups())


def signature(path: Path, version: str) -> str:
    sig = path.with_name(path.name + ".sig")
    if not sig.is_file():
        raise Refused(f"{path.name} has no signature beside it")
    text = sig.read_text().strip()
    if not text:
        raise Refused(f"{sig.name} is empty")
    try:
        decoded = base64.b64decode(text, validate=True).decode()
    except Exception:
        raise Refused(f"{sig.name} isn't a minisign signature in base64") from None
    lines = decoded.splitlines()
    if len(lines) < 4 or not lines[0].startswith("untrusted comment:") or not lines[2].startswith("trusted comment:"):
        raise Refused(f"{sig.name} isn't a minisign signature")
    fields = dict(f.split(":", 1) for f in lines[2][len("trusted comment:"):].strip().split("\t") if ":" in f)
    if fields.get("version") != version:
        raise Refused(f"{sig.name} is signed for version {fields.get('version')!r}, not {version}")
    return text


def write(version, pub_date, base_url, bundles, notes, previous=None, server_image_digest=None):
    version_key(version)
    if server_image_digest is not None and not DIGEST.match(server_image_digest):
        raise Refused(f"{server_image_digest!r} isn't a sha256 image digest")
    url = urlparse(base_url)
    if url.scheme != "https" or url.hostname != "github.com" or url.username or url.port:
        raise Refused(f"{base_url} isn't an https address on github.com")
    if previous is not None:
        old = previous.get("version", "")
        try:
            older = version_key(old) < version_key(version)
        except Refused:
            older = True  # nothing comparable was published: anything starts the channel
        if not older:
            raise Refused(f"{version} isn't newer than the published {old}")
    platforms = {}
    for kind, path in sorted(bundles):
        if kind not in PLATFORMS:
            raise Refused(f"no platform is called {kind!r}")
        if not path.is_file() or path.stat().st_size == 0:
            raise Refused(f"{path} is missing or empty")
        entry = {"signature": signature(path, version), "url": f"{base_url.rstrip('/')}/{quote(path.name)}"}
        for key in PLATFORMS[kind]:
            platforms[key] = entry
    if not platforms:
        raise Refused("no bundle was named")
    manifest = {"version": version, "notes": notes, "pub_date": pub_date, "platforms": dict(sorted(platforms.items()))}
    if server_image_digest:
        manifest["server_image"] = {"digest": server_image_digest}
    return manifest


def main(argv):
    p = argparse.ArgumentParser(description="Write the main channel's latest.json")
    p.add_argument("--version", required=True)
    p.add_argument("--pub-date", required=True)
    p.add_argument("--base-url", required=True)
    p.add_argument("--notes", default="")
    p.add_argument("--bundle", action="append", default=[], help="<darwin|windows|linux>=<file>")
    p.add_argument("--previous", type=Path)
    p.add_argument("--server-image-digest", default="", help="the server image's digest for this version, sha256:<hex>")
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args(argv)
    bundles = []
    for b in a.bundle:
        kind, _, file = b.partition("=")
        bundles.append((kind, Path(file)))
    previous = None
    if a.previous and a.previous.is_file():
        previous = json.loads(a.previous.read_text())
    try:
        manifest = write(a.version, a.pub_date, a.base_url, bundles, a.notes, previous,
                         a.server_image_digest or None)
    except Refused as e:
        print(f"write-manifest: refused: {e}", file=sys.stderr)
        return 1
    a.out.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"wrote {a.out}: {a.version}, {len(manifest['platforms'])} platform keys")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
