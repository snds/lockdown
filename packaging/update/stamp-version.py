#!/usr/bin/env python3
"""Stamps one version into every manifest the desktop build reads
(docs/plan/60-auto-update.md §2), for a "main" channel build or a dry run.

    python3 packaging/update/stamp-version.py <version> [repo root]

The version is the workspace's own (Cargo.toml) with a pre-release added,
`0.1.0-main.20261107093000`; this refuses any other base. It is written to:

- Cargo.toml's [workspace.package] version, every path dependency's
  version requirement in the workspace's Cargo.toml files, and each
  workspace crate's entry in Cargo.lock (so `--locked` builds still agree);
- apps/desktop/src-tauri/tauri.conf.json (the app's own version, which the
  updater compares, and the bundles' names);
- apps/desktop/package.json and apps/web/package.json.

The extension's manifests keep the plain version: browsers take only
dot-separated numbers there, and the extension isn't part of the channel.
A tagged release is never stamped.
"""

import json
import re
import sys
from pathlib import Path

VERSION = re.compile(r"^(\d+\.\d+\.\d+)-main\.(\d{14})$")


def stamp(root: Path, version: str) -> list[str]:
    m = VERSION.match(version)
    if not m:
        raise SystemExit(f"stamp-version: {version!r} isn't <x.y.z>-main.<YYYYMMDDHHMMSS>")
    base = m.group(1)
    changed = []

    cargo = root / "Cargo.toml"
    text = cargo.read_text()
    section = re.search(r"(?ms)^\[workspace\.package\]\n(.*?)(?=^\[)", text)
    if not section or f'version = "{base}"' not in section.group(1):
        raise SystemExit(f"stamp-version: Cargo.toml's workspace version isn't {base}")
    start, end = section.span(1)
    text = text[:start] + section.group(1).replace(f'version = "{base}"', f'version = "{version}"', 1) + text[end:]
    cargo.write_text(text)
    changed.append("Cargo.toml")

    # A path dependency's version requirement ("0.1.0") doesn't match a
    # pre-release, so each one names the stamped version too.
    path_dep = re.compile(r'^(.*\bpath = "[^"]*".*\bversion = )"' + re.escape(base) + r'"', re.M)
    for manifest in sorted(root.rglob("Cargo.toml")):
        if any(part in ("node_modules", "target") for part in manifest.relative_to(root).parts):
            continue
        before = manifest.read_text()
        after, n = path_dep.subn(lambda m: f'{m.group(1)}"{version}"', before)
        if n:
            manifest.write_text(after)
            changed.append(f"{manifest.relative_to(root)} ({n} path dependencies)")

    lock = root / "Cargo.lock"
    blocks = lock.read_text().split("[[package]]")
    count = 0
    for i, block in enumerate(blocks[1:], start=1):
        if "source = " in block:
            continue
        if f'\nversion = "{base}"\n' in block:
            blocks[i] = block.replace(f'\nversion = "{base}"\n', f'\nversion = "{version}"\n', 1)
            count += 1
    if count == 0:
        raise SystemExit("stamp-version: no workspace crate in Cargo.lock")
    lock.write_text("[[package]]".join(blocks))
    changed.append(f"Cargo.lock ({count} crates)")

    for rel in ("apps/desktop/src-tauri/tauri.conf.json", "apps/desktop/package.json", "apps/web/package.json"):
        path = root / rel
        data = json.loads(path.read_text())
        if data.get("version") != base:
            raise SystemExit(f"stamp-version: {rel} says {data.get('version')}, not {base}")
        # Only the version line changes, so the file's layout stays.
        text = path.read_text()
        text = text.replace(f'"version": "{base}"', f'"version": "{version}"', 1)
        if json.loads(text).get("version") != version:
            raise SystemExit(f"stamp-version: couldn't write the version into {rel}")
        path.write_text(text)
        changed.append(rel)
    return changed


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3):
        raise SystemExit(__doc__)
    root = Path(sys.argv[2] if len(sys.argv) == 3 else ".")
    for name in stamp(root, sys.argv[1]):
        print(f"stamped {sys.argv[1]} into {name}")
