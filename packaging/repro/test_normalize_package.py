"""Tests for normalize-package.py's deb path
(python3 packaging/repro/test_normalize_package.py). Two debs with the same files
written at different times, in different orders, normalize to the same
bytes, and keep their files. The rpm path is checked on real packages:
`rpm -K` and dnf install in release.yml, and reproducible.yml's two builds."""

import importlib.util
import io
import os
import tarfile
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("normalize", Path(__file__).with_name("normalize-package.py"))
normalize = importlib.util.module_from_spec(spec)
spec.loader.exec_module(normalize)


def tar_gz(files, mtime, order):
    out = io.BytesIO()
    with tarfile.open(fileobj=out, mode="w:gz") as t:
        for name in order:
            body = files[name]
            info = tarfile.TarInfo(name)
            info.size = len(body)
            info.mtime = mtime
            info.uid, info.uname = 1000, "builder"
            info.mode = 0o755 if name.startswith("usr/bin") else 0o644
            t.addfile(info, io.BytesIO(body))
    return out.getvalue()


def deb(path, mtime, order):
    data = {"usr/bin/lockdown-desktop": b"\x7fELF app", "usr/share/applications/Lockdown.desktop": b"[Desktop Entry]\n"}
    md5 = "".join(f"{normalize.hashlib.md5(data[n]).hexdigest()}  {n}\n" for n in order)
    control = {"control": b"Package: lockdown\n", "md5sums": md5.encode()}
    members = [
        ("debian-binary", b"2.0\n"),
        ("control.tar.gz", tar_gz(control, mtime, ["md5sums", "control"] if order[0].startswith("usr/bin") else ["control", "md5sums"])),
        ("data.tar.gz", tar_gz(data, mtime, order)),
    ]
    Path(path).write_bytes(normalize.write_ar(members, mtime))


class NormalizeDeb(unittest.TestCase):
    def test_two_builds_become_the_same_bytes_with_the_same_files(self):
        with tempfile.TemporaryDirectory() as d:
            a, b = os.path.join(d, "a.deb"), os.path.join(d, "b.deb")
            names = ["usr/bin/lockdown-desktop", "usr/share/applications/Lockdown.desktop"]
            deb(a, 1_800_000_000, names)
            deb(b, 1_800_000_777, list(reversed(names)))
            self.assertNotEqual(Path(a).read_bytes(), Path(b).read_bytes())
            for p in (a, b):
                normalize.normalize_deb(p, 1_700_000_000)
            self.assertEqual(Path(a).read_bytes(), Path(b).read_bytes())
            members = dict(normalize.read_ar(Path(a).read_bytes()))
            data = tarfile.open(fileobj=io.BytesIO(members["data.tar.gz"]), mode="r:gz")
            infos = data.getmembers()
            self.assertEqual([i.name for i in infos], sorted(names))
            self.assertTrue(all(i.mtime == 1_700_000_000 and i.uid == 0 and i.uname == "root" for i in infos))
            self.assertEqual(data.extractfile("usr/bin/lockdown-desktop").read(), b"\x7fELF app")
            self.assertEqual(data.getmember("usr/bin/lockdown-desktop").mode, 0o755)
            control = tarfile.open(fileobj=io.BytesIO(members["control.tar.gz"]), mode="r:gz")
            md5 = control.extractfile("md5sums").read().decode().splitlines()
            self.assertEqual([l.split("  ")[1] for l in md5], sorted(names))


if __name__ == "__main__":
    unittest.main()
