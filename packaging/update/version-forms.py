#!/usr/bin/env python3
"""One build's version in the form each package format takes
(docs/plan/60-auto-update.md §2), and the check that every form is valid.

    version-forms.py check <version>             every form, checked; fails on one that isn't valid
    version-forms.py rpm-config <version>        the Tauri config that bundles the rpm (JSON)
    version-forms.py rewrite-deb <deb> <version> sets the deb's control Version to the deb form

The app, Cargo, npm and the updater take semver: `0.1.0-main.<YYYYMMDDHHMMSS>`
on the main channel, plain `0.1.0` on a tag. The packages that people's
package managers order need their own form, so that a main build sorts
after the main builds before it and before the release of its version:

| Format       | Main channel                         | Tag    |
|--------------|--------------------------------------|--------|
| semver (app) | 0.1.0-main.20261107025710            | 0.1.0  |
| deb Version  | 0.1.0~main.20261107025710            | 0.1.0  |
| rpm          | Version 0.1.0, Release 0.main.20261107025710 | 0.1.0, Release 1 |
| MSIX         | 0.1.0.0                              | 0.1.0.0 |
| extension    | 0.1.0                                | 0.1.0  |

deb: `-` would split `0.1.0` (upstream) from `main.…` (revision), which
sorts after a later plain `0.1.0`; `~` sorts before. rpm: `-` is refused in
Version and Tauri takes its rpm Version from the app's (semver can't hold
`~`), so the pre-release goes in Release, the Fedora convention for one
(`0.<tag>` sorts before the release's `1`). MSI isn't built on the channel.
"""

import importlib.util
import io
import json
import re
import sys
import tarfile
from pathlib import Path

SEMVER = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:-main\.(\d{14}))?$")
DEB = re.compile(r"^[0-9][A-Za-z0-9.+~]*$")  # no revision, so no '-'
RPM_FIELD = re.compile(r"^[A-Za-z0-9._+%{}~^]+$")
MSIX = re.compile(r"^\d+\.\d+\.\d+\.\d+$")
EXTENSION = re.compile(r"^\d+(\.\d+){0,3}$")


class Invalid(Exception):
    pass


def parse(version):
    m = SEMVER.match(version)
    if not m:
        raise Invalid(f"{version!r} is neither x.y.z nor x.y.z-main.<YYYYMMDDHHMMSS>")
    base = ".".join(m.group(i) for i in (1, 2, 3))
    return base, m.group(4)


def forms(version):
    base, stamp = parse(version)
    return {
        "semver": version,
        "deb": f"{base}~main.{stamp}" if stamp else base,
        "rpm_version": base,
        "rpm_release": f"0.main.{stamp}" if stamp else "1",
        "msix": f"{base}.0",
        "extension": base,
    }


def check(version):
    f = forms(version)
    problems = []
    if not DEB.match(f["deb"]):
        problems.append(f"deb {f['deb']!r}")
    for key in ("rpm_version", "rpm_release"):
        if not RPM_FIELD.match(f[key]):
            problems.append(f"rpm {key} {f[key]!r}")
    if not MSIX.match(f["msix"]) or any(int(p) > 65535 for p in f["msix"].split(".")):
        problems.append(f"MSIX {f['msix']!r}")
    if not EXTENSION.match(f["extension"]) or any(int(p) > 65535 for p in f["extension"].split(".")):
        problems.append(f"extension {f['extension']!r}")
    if problems:
        raise Invalid("not valid: " + ", ".join(problems))
    return f


def rpm_config(version):
    f = forms(version)
    return {"version": f["rpm_version"], "bundle": {"linux": {"rpm": {"release": f["rpm_release"]}}}}


def _normalizer():
    path = Path(__file__).resolve().parent.parent / "repro" / "normalize-package.py"
    spec = importlib.util.spec_from_file_location("normalize_package", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def rewrite_deb(path, version):
    """Sets `Version:` in the deb's control file. normalize-package.py runs
    after this and fixes the members' times and order."""
    want = forms(version)["deb"]
    norm = _normalizer()
    data = Path(path).read_bytes()
    members = norm.read_ar(data)
    out = []
    changed = False
    for name, body in members:
        if name.startswith("control.tar"):
            src = tarfile.open(fileobj=io.BytesIO(body), mode="r:gz")
            buf = io.BytesIO()
            dst = tarfile.open(fileobj=buf, mode="w", format=tarfile.GNU_FORMAT)
            for info in src.getmembers():
                blob = src.extractfile(info).read() if info.isfile() else None
                if info.isfile() and info.name.lstrip("./") == "control":
                    text = blob.decode()
                    new = re.sub(r"(?m)^Version: .*$", f"Version: {want}", text, count=1)
                    if new == text and f"Version: {want}" not in text:
                        raise Invalid("the deb's control file has no Version line")
                    blob = new.encode()
                    info.size = len(blob)
                    changed = True
                dst.addfile(info, io.BytesIO(blob) if blob is not None else None)
            dst.close()
            body = norm.gzip_bytes(buf.getvalue())
        out.append((name, body))
    if not changed:
        raise Invalid("the deb has no control file")
    Path(path).write_bytes(norm.write_ar(out, 0))
    return want


def main(argv):
    try:
        if argv[:1] == ["check"] and len(argv) == 2:
            for k, v in check(argv[1]).items():
                print(f"{k}: {v}")
        elif argv[:1] == ["rpm-config"] and len(argv) == 2:
            print(json.dumps(rpm_config(argv[1])))
        elif argv[:1] == ["rewrite-deb"] and len(argv) == 3:
            print(f"{argv[1]}: Version {rewrite_deb(argv[1], argv[2])}")
        else:
            print(__doc__, file=sys.stderr)
            return 2
    except Invalid as e:
        print(f"version-forms: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
