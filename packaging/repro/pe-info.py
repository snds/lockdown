#!/usr/bin/env python3
"""Prints what a Windows executable records about its own build, for
reproducible.yml's macOS and Windows report (docs/plan/29-release-polish.md
§1): the linker's version, the PE time stamp, and the CodeView record (the
PDB's GUID, age and path) the linker writes into the debug directory.

    pe-info.py <file.exe>...
"""

import struct
import sys


def info(path):
    data = open(path, "rb").read()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe : pe + 4] != b"PE\0\0":
        return f"{path}: not a PE file"
    stamp = struct.unpack_from("<I", data, pe + 8)[0]
    opt = pe + 24
    major, minor = data[opt + 2], data[opt + 3]
    rsds = data.find(b"RSDS")
    pdb = "no CodeView record"
    if rsds >= 0:
        guid = data[rsds + 4 : rsds + 20].hex()
        age = struct.unpack_from("<I", data, rsds + 20)[0]
        end = data.index(b"\0", rsds + 24)
        pdb = f"pdb guid {guid} age {age} path {data[rsds + 24 : end].decode(errors='replace')}"
    return f"{path}: linker {major}.{minor}, time stamp {stamp:#010x}, {pdb}"


for p in sys.argv[1:]:
    print(info(p))
