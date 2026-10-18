#!/usr/bin/env python3
"""Prints the SHA-256 of one file inside an OCI image archive.

    python3 packaging/repro/oci-file-hash.py <image.oci.tar> <path in image>

Reads index.json, follows the (single-platform) image manifest to its
layers, and returns the hash of the file from the topmost layer that holds
it. Used to show the server binary in the image is byte for byte the one
published on its own (docs/plan/24-release.md §5). Standard library only.
"""

import gzip
import hashlib
import io
import json
import sys
import tarfile


def blob(archive: tarfile.TarFile, digest: str) -> bytes:
    algo, hexdigest = digest.split(":", 1)
    member = archive.extractfile(f"blobs/{algo}/{hexdigest}")
    if member is None:
        raise SystemExit(f"oci-file-hash: blob {digest} missing")
    return member.read()


def manifest_of(archive: tarfile.TarFile, descriptor: dict) -> dict:
    doc = json.loads(blob(archive, descriptor["digest"]))
    if "manifests" in doc:  # an index inside the index: take the amd64 one
        for m in doc["manifests"]:
            if m.get("platform", {}).get("architecture", "amd64") == "amd64":
                return manifest_of(archive, m)
        raise SystemExit("oci-file-hash: no amd64 manifest")
    return doc


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    path = sys.argv[2].lstrip("/")
    with tarfile.open(sys.argv[1]) as archive:
        index = json.loads(archive.extractfile("index.json").read())
        manifest = manifest_of(archive, index["manifests"][0])
        found = None
        for layer in manifest["layers"]:
            data = blob(archive, layer["digest"])
            if data[:2] == b"\x1f\x8b":
                data = gzip.decompress(data)
            with tarfile.open(fileobj=io.BytesIO(data)) as t:
                for member in t.getmembers():
                    if member.name.lstrip("./") == path and member.isfile():
                        found = hashlib.sha256(t.extractfile(member).read()).hexdigest()
        if found is None:
            raise SystemExit(f"oci-file-hash: {path} is not in the image")
        print(found)


if __name__ == "__main__":
    main()
