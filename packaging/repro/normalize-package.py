#!/usr/bin/env python3
"""Makes a deb or rpm from Tauri's bundler the same bytes on every build of
one commit (docs/plan/29-release-polish.md §1). Python's standard library
only; run inside the pinned build container (packaging/repro/build-desktop.sh)
so the zlib that recompresses is the same on every runner.

    normalize-package.py <file.deb | file.rpm>...

SOURCE_DATE_EPOCH (the commit's time) must be set. Each file is rewritten
in place. What the bundler leaves that differs between two builds of the
same binaries, and what this does about it:

deb (an ar archive of debian-binary, control.tar.gz, data.tar.gz):
  - ar member times, owners and modes: set to SOURCE_DATE_EPOCH, 0, 0, 0644.
  - tar entry order (the bundler walks the folder in the file system's
    order): sorted by name.
  - tar entry times and owners: SOURCE_DATE_EPOCH, root:root (0:0).
  - md5sums lines (the same walk): sorted by path.
  - gzip: level 6 as the bundler uses, no name, time 0.

rpm (lead, signature header, header, gzip'd cpio payload):
  - RPMTAG_BUILDTIME and every RPMTAG_FILEMTIMES: SOURCE_DATE_EPOCH.
  - cpio entry times: SOURCE_DATE_EPOCH; the payload recompressed (gzip
    level 6, no name, time 0).
  - Everything that covers those bytes recomputed in place: the payload
    digests in the header (SHA-256, SHA-512, SHA3-256, compressed and not),
    the header digests in the signature header (SHA-1, SHA-256, SHA3-256)
    and the header-plus-payload size. Every field keeps its length, so no
    offset moves. An rpm with a tag this doesn't know how to keep true
    (a signature, a build host) is refused rather than half-rewritten.
"""

import gzip
import hashlib
import io
import os
import struct
import sys
import tarfile
import zlib

GZIP_LEVEL = 6


def epoch():
    value = os.environ.get("SOURCE_DATE_EPOCH", "")
    if not value.isdigit():
        sys.exit("normalize-package: SOURCE_DATE_EPOCH must be set to the commit's time")
    return int(value)


def gzip_bytes(data):
    """gzip with a fixed header: no file name, time 0."""
    out = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=out, compresslevel=GZIP_LEVEL, mtime=0) as f:
        f.write(data)
    return out.getvalue()


# --- deb ---------------------------------------------------------------------


def read_ar(data):
    if not data.startswith(b"!<arch>\n"):
        raise ValueError("not an ar archive")
    pos, members = 8, []
    while pos < len(data):
        head = data[pos : pos + 60]
        if len(head) < 60 or head[58:60] != b"`\n":
            raise ValueError("a broken ar member header")
        name = head[0:16].decode().strip().rstrip("/")
        size = int(head[48:58].decode().strip())
        body = data[pos + 60 : pos + 60 + size]
        members.append((name, body))
        pos += 60 + size + (size & 1)
    return members


def write_ar(members, when):
    out = bytearray(b"!<arch>\n")
    for name, body in members:
        if len(name) > 16:
            raise ValueError(f"ar member name {name!r} is too long")
        head = (
            name.ljust(16)
            + str(when).ljust(12)
            + "0".ljust(6)
            + "0".ljust(6)
            + "100644".ljust(8)
            + str(len(body)).ljust(10)
            + "`\n"
        )
        out += head.encode()
        out += body
        if len(body) & 1:
            out += b"\n"
    return bytes(out)


def retar(blob, when, sort_md5sums):
    src = tarfile.open(fileobj=io.BytesIO(blob), mode="r:*")
    entries = []
    for info in src.getmembers():
        body = src.extractfile(info).read() if info.isfile() else None
        if sort_md5sums and info.isfile() and info.name.lstrip("./") == "md5sums":
            lines = sorted(body.decode().splitlines(keepends=True), key=lambda l: l.split("  ", 1)[-1])
            body = "".join(lines).encode()
            info.size = len(body)
        info.mtime = when
        info.uid = info.gid = 0
        info.uname = info.gname = "root"
        info.mode &= 0o7777
        info.pax_headers = {}
        entries.append((info, body))
    entries.sort(key=lambda e: e[0].name)
    out = io.BytesIO()
    with tarfile.open(fileobj=out, mode="w", format=tarfile.GNU_FORMAT) as dst:
        for info, body in entries:
            dst.addfile(info, io.BytesIO(body) if body is not None else None)
    return gzip_bytes(out.getvalue())


def normalize_deb(path, when):
    members = read_ar(open(path, "rb").read())
    names = [n for n, _ in members]
    if names[:1] != ["debian-binary"] or len(names) != 3:
        raise ValueError(f"unexpected deb members {names}")
    fixed = [("debian-binary", members[0][1])]
    for name, body in members[1:]:
        if not name.endswith(".tar.gz"):
            raise ValueError(f"{name}: only gzip'd tar members are handled")
        fixed.append((name, retar(body, when, sort_md5sums=name.startswith("control."))))
    open(path, "wb").write(write_ar(fixed, when))


# --- rpm ---------------------------------------------------------------------

HEADER_MAGIC = b"\x8e\xad\xe8\x01\x00\x00\x00\x00"
INT32, INT64, STRING, BIN, STRING_ARRAY = 4, 5, 6, 7, 8

# The main header's tags this rewrites, and the tags it refuses to keep.
BUILDTIME, BUILDHOST, FILEMTIMES = 1006, 1007, 1034
PAYLOADCOMPRESSOR = 1125
PAYLOAD_DIGESTS = {
    5092: ("sha256", True),  # RPMTAG_PAYLOADSHA256 (string array)
    5097: ("sha256", False),  # RPMTAG_PAYLOADSHA256ALT
    5121: ("sha512", True),
    5122: ("sha512", False),
    5123: ("sha3_256", True),
    5124: ("sha3_256", False),
}
# Signature header.
SIG_DIGESTS = {269: "sha1", 273: "sha256", 279: "sha3_256"}  # the header's SHA-1, SHA-256, SHA3-256
SIG_SIZE, SIG_LONGSIZE = 1000, 270
SIG_KEEP = {1007, 271, 1008, 999, 62}  # payload size, long archive size, reserved space, reserved, region
SIG_SIGNATURES = {267, 268, 278, 1002, 1005}  # DSA, RSA, OpenPGP, PGP, GPG: a signed rpm is refused


class Header:
    """One rpm header: index entries and the store, parsed in place."""

    def __init__(self, data, start):
        if data[start : start + 8] != HEADER_MAGIC:
            raise ValueError("no rpm header magic")
        self.data = data
        self.start = start
        count, self.store_size = struct.unpack(">II", data[start + 8 : start + 16])
        self.entries = {}
        for i in range(count):
            at = start + 16 + 16 * i
            tag, kind, offset, n = struct.unpack(">IIiI", data[at : at + 16])
            self.entries[tag] = (kind, offset, n)
        self.store = start + 16 + 16 * count
        self.end = self.store + self.store_size

    def where(self, tag):
        kind, offset, n = self.entries[tag]
        return kind, self.store + offset, n

    def ints(self, tag, size):
        kind, at, n = self.where(tag)
        fmt = ">%d%s" % (n, "I" if size == 4 else "Q")
        return list(struct.unpack(fmt, self.data[at : at + size * n]))

    def set_ints(self, tag, values, size):
        kind, at, n = self.where(tag)
        if len(values) != n:
            raise ValueError(f"tag {tag}: {len(values)} values for {n}")
        fmt = ">%d%s" % (n, "I" if size == 4 else "Q")
        self.data[at : at + size * n] = struct.pack(fmt, *values)

    def string(self, tag):
        kind, at, n = self.where(tag)
        end = self.data.index(b"\0", at)
        return self.data[at:end].decode()

    def set_string(self, tag, value):
        kind, at, n = self.where(tag)
        old = self.string(tag)
        if len(value) != len(old):
            raise ValueError(f"tag {tag}: {value!r} isn't the length of {old!r}")
        self.data[at : at + len(value)] = value.encode()


def rewrite_cpio(raw, when):
    """Sets every newc entry's mtime to `when`, in place; sizes don't change."""
    raw = bytearray(raw)
    pos = 0
    while True:
        if raw[pos : pos + 6] not in (b"070701", b"070702"):
            raise ValueError(f"no cpio entry at {pos}")
        fields = [int(raw[pos + 6 + 8 * i : pos + 14 + 8 * i], 16) for i in range(13)]
        filesize, namesize = fields[6], fields[11]
        raw[pos + 6 + 8 * 5 : pos + 14 + 8 * 5] = b"%08X" % when
        name_end = pos + 110 + namesize
        name = bytes(raw[pos + 110 : name_end - 1])
        data_start = (name_end + 3) & ~3
        pos = (data_start + filesize + 3) & ~3
        if name == b"TRAILER!!!":
            return bytes(raw)


def normalize_rpm(path, when):
    data = bytearray(open(path, "rb").read())
    if data[:4] != b"\xed\xab\xee\xdb":
        raise ValueError("not an rpm")
    sig = Header(data, 96)
    main_start = sig.end + ((8 - (sig.end % 8)) % 8)
    main = Header(data, main_start)
    payload_at = main.end

    for tag in sig.entries:
        if tag in SIG_SIGNATURES:
            raise ValueError(f"signature tag {tag}: a signed rpm can't be normalized; normalize, then sign")
        if tag not in SIG_DIGESTS and tag not in (SIG_SIZE, SIG_LONGSIZE) and tag not in SIG_KEEP:
            raise ValueError(f"signature tag {tag} isn't one this keeps true")
    if BUILDHOST in main.entries:
        raise ValueError("the rpm names its build host; build without one")
    compressor = main.string(PAYLOADCOMPRESSOR) if PAYLOADCOMPRESSOR in main.entries else "gzip"
    if compressor != "gzip":
        raise ValueError(f"payload compressor {compressor!r}: only gzip is handled")

    raw = zlib.decompress(bytes(data[payload_at:]), 16 + zlib.MAX_WBITS)
    raw = rewrite_cpio(raw, when)
    payload = gzip_bytes(raw)

    main.set_ints(BUILDTIME, [when], 4)
    if FILEMTIMES in main.entries:
        main.set_ints(FILEMTIMES, [when] * len(main.ints(FILEMTIMES, 4)), 4)
    for tag, (algo, compressed) in PAYLOAD_DIGESTS.items():
        if tag in main.entries:
            digest = hashlib.new(algo, payload if compressed else raw).hexdigest()
            main.set_string(tag, digest)

    header_bytes = bytes(data[main_start:payload_at])
    for tag, algo in SIG_DIGESTS.items():
        if tag in sig.entries:
            sig.set_string(tag, hashlib.new(algo, header_bytes).hexdigest())
    if SIG_SIZE in sig.entries:
        sig.set_ints(SIG_SIZE, [len(header_bytes) + len(payload)], 4)
    if SIG_LONGSIZE in sig.entries:
        sig.set_ints(SIG_LONGSIZE, [len(header_bytes) + len(payload)], 8)

    open(path, "wb").write(bytes(data[:payload_at]) + payload)


def main(argv):
    if len(argv) < 2:
        sys.exit(__doc__)
    when = epoch()
    for path in argv[1:]:
        if path.endswith(".deb"):
            normalize_deb(path, when)
        elif path.endswith(".rpm"):
            normalize_rpm(path, when)
        else:
            sys.exit(f"normalize-package: {path}: not a .deb or .rpm")
        print(f"normalize-package: {path}: {hashlib.sha256(open(path, 'rb').read()).hexdigest()}")


if __name__ == "__main__":
    main(sys.argv)
