#!/usr/bin/env python3
"""Writes the server's `latest.json` for the shared feed
(docs/plan/66-shared-feed.md §3): the rolling `lockdown-server-main`
pre-release on snds/releases, in the shape every server's feed has.

    python3 packaging/update/write-server-manifest.py \\
        --version 0.1.0-main.20261107093000 --commit <40 hex> \\
        --image ghcr.io/snds/lockdown-server@sha256:<64 hex> \\
        [--previous published.json] --out feed/latest.json

    {"app": "lockdown-server", "version": "0.1.0-main.20261107093000",
     "commit": "<40 hex>", "pubDate": "2026-11-07T09:30:00Z",
     "images": {"server": {"repository": "ghcr.io/snds/lockdown-server",
                           "digest": "sha256:<64 hex>"}}}

release.yml's `server-feed` job runs it after `server-image` pushed the
image, so the manifest never names an image that isn't in the registry.

Exit codes: 0, the manifest is written. 3, this exact version is already
published (a rerun). 4, the published version is newer (a late run). 2, an
input is wrong, or the published manifest can't be read. Only 0 writes
anything, so the feed never goes backwards.
"""

import argparse
import json
import re
import sys
from pathlib import Path

APP = "lockdown-server"
IMAGE_NAME = "server"
# The one image a Lockdown server runs (bins/server/src/updates.rs, `IMAGE`).
REPOSITORY = "ghcr.io/snds/lockdown-server"

VERSION = re.compile(r"^(\d+)\.(\d+)\.(\d+)-main\.(\d{14})$")
RELEASE = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")

ALREADY, NEWER_PUBLISHED, BAD = 3, 4, 2


class Refused(Exception):
    """The manifest is not written. `code` is the exit code."""

    def __init__(self, code, why):
        super().__init__(why)
        self.code = code


def order(version):
    """A key that sorts versions as semver does: a release follows its own main builds."""
    m = VERSION.match(version)
    if m:
        return (int(m[1]), int(m[2]), int(m[3]), 0, int(m[4]))
    m = RELEASE.match(version)
    if m:
        return (int(m[1]), int(m[2]), int(m[3]), 1, 0)
    raise Refused(BAD, f"{version!r} isn't x.y.z or x.y.z-main.<YYYYMMDDHHMMSS>")


def pub_date(version):
    """The commit time the version carries, as the feed writes it."""
    t = VERSION.match(version)[4]
    return f"{t[0:4]}-{t[4:6]}-{t[6:8]}T{t[8:10]}:{t[10:12]}:{t[12:14]}Z"


def write(version, commit, image, previous=None):
    """The manifest, or `Refused`. `previous` is the published manifest's text, if there is one."""
    if not VERSION.match(version):
        raise Refused(BAD, f"{version!r} isn't <x.y.z>-main.<YYYYMMDDHHMMSS>")
    if not COMMIT.match(commit):
        raise Refused(BAD, f"{commit!r} isn't a commit")
    repository, at, digest = image.partition("@")
    if repository != REPOSITORY or not at or not DIGEST.match(digest):
        raise Refused(BAD, f"{image!r} isn't {REPOSITORY}@sha256:<64 hex>")
    if previous is not None:
        try:
            published = json.loads(previous)["version"]
            if not isinstance(published, str):
                raise TypeError
        except (ValueError, KeyError, TypeError):
            # Never guess past a manifest that can't be read: a person looks first.
            raise Refused(BAD, "the published manifest can't be read; nothing is written over it") from None
        was, now = order(published), order(version)
        if was == now:
            raise Refused(ALREADY, f"{version} is already published")
        if was > now:
            raise Refused(NEWER_PUBLISHED, f"the published {published} is newer than {version}")
    return {
        "app": APP,
        "version": version,
        "commit": commit,
        "pubDate": pub_date(version),
        "images": {IMAGE_NAME: {"repository": repository, "digest": digest}},
    }


def main(argv):
    p = argparse.ArgumentParser(description="Write the server feed's latest.json")
    p.add_argument("--version", required=True)
    p.add_argument("--commit", required=True)
    p.add_argument("--image", required=True, help=f"{REPOSITORY}@sha256:<64 hex>")
    p.add_argument("--previous", type=Path, help="the published latest.json, if there is one")
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args(argv)
    previous = None
    if a.previous and a.previous.is_file():
        previous = a.previous.read_text()
    try:
        manifest = write(a.version, a.commit, a.image, previous)
    except Refused as e:
        print(f"write-server-manifest: refused: {e}", file=sys.stderr)
        return e.code
    a.out.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"wrote {a.out}: {a.version}, {manifest['images'][IMAGE_NAME]['digest']}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
