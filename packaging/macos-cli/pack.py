#!/usr/bin/env python3
"""Packs the signed `lockdown` command for a Mac download
(docs/plan/46-mac-cli-download.md §2).

    python3 packaging/macos-cli/pack.py <binary> <version> <out dir>

Writes <out>/lockdown-<version>-macos-universal.tar.gz holding one folder,
lockdown-<version>-macos-universal/, with:

    lockdown     the universal binary, as signed (0755)
    LICENSE      the Apache License 2.0
    NOTICE       the copyright notice
    README.txt   installing it, the first run, Gatekeeper, where files go

The archive is the same bytes for the same inputs: entries in that order,
every time the commit's (SOURCE_DATE_EPOCH, else 0), owner and group 0
with no names, fixed modes, ustar, and a gzip header without a name or
time. Prints the archive's path.
"""

import gzip
import io
import os
import sys
import tarfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def entry(name, size, mode, mtime, kind=tarfile.REGTYPE):
    info = tarfile.TarInfo(name)
    info.size = size
    info.mode = mode
    info.mtime = mtime
    info.type = kind
    info.uid = info.gid = 0
    info.uname = info.gname = ""
    return info


def main(argv):
    if len(argv) != 4:
        sys.stderr.write("usage: pack.py <binary> <version> <out dir>\n")
        return 2
    binary, version, out = argv[1], argv[2], argv[3]
    mtime = int(os.environ.get("SOURCE_DATE_EPOCH") or 0)
    base = f"lockdown-{version}-macos-universal"

    with open(os.path.join(ROOT, "packaging/macos-cli/README.txt"), encoding="utf-8") as f:
        readme = f.read().replace("@VERSION@", version).encode("utf-8")
    if b"@VERSION@" in readme:
        sys.stderr.write("pack.py: README.txt still has a placeholder\n")
        return 1

    files = []
    with open(binary, "rb") as f:
        files.append(("lockdown", f.read(), 0o755))
    for name in ("LICENSE", "NOTICE"):
        with open(os.path.join(ROOT, name), "rb") as f:
            files.append((name, f.read(), 0o644))
    files.append(("README.txt", readme, 0o644))

    os.makedirs(out, exist_ok=True)
    path = os.path.join(out, f"{base}.tar.gz")
    with open(path, "wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, compresslevel=9, mtime=0) as gz:
            with tarfile.open(fileobj=gz, mode="w", format=tarfile.USTAR_FORMAT) as tar:
                tar.addfile(entry(base + "/", 0, 0o755, mtime, tarfile.DIRTYPE))
                for name, data, mode in files:
                    tar.addfile(entry(f"{base}/{name}", len(data), mode, mtime), io.BytesIO(data))
    print(path)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
