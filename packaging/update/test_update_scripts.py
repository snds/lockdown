#!/usr/bin/env python3
"""Tests for the main channel's scripts (docs/plan/60-auto-update.md §7):
the manifest writer and the version stamp. Run from the repository root:

    python3 packaging/update/test_update_scripts.py
"""

import base64
import importlib.util
import io
import json
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), HERE / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


wm = load("write-manifest")
sv = load("stamp-version")
vf = load("version-forms")

V1 = "0.1.0-main.20261107093000"
V2 = "0.1.0-main.20261107120000"
BASE = "https://github.com/snds/lockdown/releases/download/main-build"


def sig_text(version, file="bundle"):
    """A signature in the shape `tauri signer sign --app-version` writes (the writer checks the shape and the version; the app checks the signature)."""
    comment = f"timestamp:1794017157\tfile:{file}"
    if version is not None:
        comment += f"\tversion:{version}"
    text = f"untrusted comment: signature from tauri secret key\nRUQAAAAA\ntrusted comment: {comment}\nAAAA\n"
    return base64.b64encode(text.encode()).decode()


class ManifestWriter(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)

    def bundle(self, name, version=V1, sig=True):
        path = self.dir / name
        path.write_bytes(b"bundle bytes")
        if sig:
            (self.dir / f"{name}.sig").write_text(sig_text(version, name))
        return path

    def all_three(self, version=V1):
        return [
            ("darwin", self.bundle(f"Lockdown_{version}_universal.app.tar.gz", version)),
            ("windows", self.bundle(f"Lockdown_{version}_x64-setup.exe", version)),
            ("linux", self.bundle(f"lockdown_{version}_amd64.deb", version)),
        ]

    def test_writes_every_platform_key_the_app_reads(self):
        m = wm.write(V1, "2026-11-07T09:30:00Z", BASE, self.all_three(), "notes")
        self.assertEqual(m["version"], V1)
        self.assertEqual(
            sorted(m["platforms"]),
            sorted(["darwin-aarch64", "darwin-x86_64", "darwin-aarch64-app", "darwin-x86_64-app",
                    "windows-x86_64", "windows-x86_64-nsis", "linux-x86_64", "linux-x86_64-deb"]),
        )
        mac = m["platforms"]["darwin-aarch64"]
        self.assertEqual(mac, m["platforms"]["darwin-x86_64"], "one universal bundle")
        self.assertEqual(mac["url"], f"{BASE}/Lockdown_{V1}_universal.app.tar.gz")
        self.assertEqual(mac["signature"], sig_text(V1, f"Lockdown_{V1}_universal.app.tar.gz"))
        self.assertTrue(m["platforms"]["windows-x86_64"]["url"].endswith("_x64-setup.exe"))
        # The output is what the plugin parses: plain JSON, stable order.
        self.assertEqual(json.loads(json.dumps(m)), m)
        self.assertEqual(list(m["platforms"]), sorted(m["platforms"]))

    def test_refuses_a_version_outside_the_scheme(self):
        for bad in ["0.1.0", "0.1.0-main.2026110709", "v0.1.0-main.20261107093000", "0.1.0-beta.20261107093000"]:
            with self.assertRaises(wm.Refused, msg=bad):
                wm.write(bad, "", BASE, self.all_three(), "")

    def test_refuses_a_base_url_off_github_or_off_https(self):
        for bad in ["http://github.com/x", "https://example.org/x", "https://github.com.evil.example/x", "https://u@github.com/x", "https://github.com:8443/x"]:
            with self.assertRaises(wm.Refused, msg=bad):
                wm.write(V1, "", bad, self.all_three(), "")

    def test_refuses_a_missing_empty_or_foreign_signature(self):
        with self.assertRaises(wm.Refused):
            wm.write(V1, "", BASE, [("darwin", self.bundle("a.app.tar.gz", sig=False))], "")
        empty = self.bundle("b.app.tar.gz")
        (self.dir / "b.app.tar.gz.sig").write_text("")
        with self.assertRaises(wm.Refused):
            wm.write(V1, "", BASE, [("darwin", empty)], "")
        junk = self.bundle("c.app.tar.gz")
        (self.dir / "c.app.tar.gz.sig").write_text("not base64 at all!")
        with self.assertRaises(wm.Refused):
            wm.write(V1, "", BASE, [("darwin", junk)], "")

    def test_refuses_a_signature_for_another_version_or_none(self):
        with self.assertRaisesRegex(wm.Refused, "signed for version"):
            wm.write(V2, "", BASE, [("darwin", self.bundle("d.app.tar.gz", version=V1))], "")
        with self.assertRaisesRegex(wm.Refused, "signed for version"):
            wm.write(V1, "", BASE, [("darwin", self.bundle("e.app.tar.gz", version=None))], "")

    def test_refuses_an_unknown_platform_an_empty_bundle_or_none(self):
        with self.assertRaises(wm.Refused):
            wm.write(V1, "", BASE, [("android", self.bundle("f.apk"))], "")
        empty = self.bundle("g.exe")
        empty.write_bytes(b"")
        with self.assertRaises(wm.Refused):
            wm.write(V1, "", BASE, [("windows", empty)], "")
        with self.assertRaises(wm.Refused):
            wm.write(V1, "", BASE, [], "")

    def test_never_takes_the_channel_backwards(self):
        bundles = self.all_three(V1)
        with self.assertRaisesRegex(wm.Refused, "isn't newer"):
            wm.write(V1, "", BASE, bundles, "", previous={"version": V2})
        with self.assertRaisesRegex(wm.Refused, "isn't newer"):
            wm.write(V1, "", BASE, bundles, "", previous={"version": V1})
        self.assertEqual(wm.write(V1, "", BASE, bundles, "", previous={"version": "0.1.0-main.20261101000000"})["version"], V1)
        # Nothing comparable published yet: the first run starts the channel.
        self.assertEqual(wm.write(V1, "", BASE, bundles, "", previous={})["version"], V1)

    def test_names_the_server_image_digest_when_given(self):
        digest = "sha256:" + "ab" * 32
        m = wm.write(V1, "2026-11-07T09:30:00Z", BASE, self.all_three(), "", server_image_digest=digest)
        self.assertEqual(m["server_image"], {"digest": digest})
        self.assertNotIn("server_image", wm.write(V1, "2026-11-07T09:30:00Z", BASE, self.all_three(), ""))
        for bad in ["sha256:abc", "sha512:" + "ab" * 32, "sha256:" + "AB" * 32, "ab" * 32]:
            with self.assertRaises(wm.Refused, msg=bad):
                wm.write(V1, "2026-11-07T09:30:00Z", BASE, self.all_three(), "", server_image_digest=bad)

    def test_the_command_line_writes_the_file_and_fails_with_a_reason(self):
        out = self.dir / "latest.json"
        args = ["--version", V1, "--pub-date", "2026-11-07T09:30:00Z", "--base-url", BASE, "--out", str(out)]
        bundles = sum((["--bundle", f"{k}={p}"] for k, p in self.all_three()), [])
        self.assertEqual(wm.main(args + bundles), 0)
        self.assertEqual(json.loads(out.read_text())["version"], V1)
        self.assertNotIn("server_image", json.loads(out.read_text()))
        digest = "sha256:" + "cd" * 32
        self.assertEqual(wm.main(args + bundles + ["--server-image-digest", digest]), 0)
        self.assertEqual(json.loads(out.read_text())["server_image"]["digest"], digest)
        # An empty digest (the image job didn't run) writes the manifest without one.
        self.assertEqual(wm.main(args + bundles + ["--server-image-digest", ""]), 0)
        self.assertNotIn("server_image", json.loads(out.read_text()))
        previous = self.dir / "old.json"
        previous.write_text(json.dumps({"version": V2}))
        self.assertEqual(wm.main(args + bundles + ["--previous", str(previous)]), 1)


class VersionForms(unittest.TestCase):
    def test_each_format_gets_a_valid_form(self):
        f = vf.check(V1)
        self.assertEqual(f["semver"], V1)
        self.assertEqual(f["deb"], "0.1.0~main.20261107093000")
        self.assertEqual((f["rpm_version"], f["rpm_release"]), ("0.1.0", "0.main.20261107093000"))
        self.assertEqual(f["msix"], "0.1.0.0")
        self.assertEqual(f["extension"], "0.1.0")
        tag = vf.check("0.1.0")
        self.assertEqual((tag["deb"], tag["rpm_version"], tag["rpm_release"]), ("0.1.0", "0.1.0", "1"))
        self.assertEqual(vf.rpm_config(V1), {"version": "0.1.0", "bundle": {"linux": {"rpm": {"release": "0.main.20261107093000"}}}})

    def test_refuses_what_a_format_can_not_take(self):
        for bad in ["0.1.0-beta.1", "0.1.0-main.1", "v0.1.0", "0.1", "0.1.0-main.20261107093000+x"]:
            with self.assertRaises(vf.Invalid, msg=bad):
                vf.check(bad)
        with self.assertRaises(vf.Invalid):
            vf.check("0.1.70000")  # MSIX and the extension take at most 65535

    def test_package_managers_order_main_builds_before_the_release(self):
        older, newer = "0.1.0-main.20261107093000", "0.1.0-main.20261107120000"
        if shutil.which("dpkg"):
            for a, b in [(older, newer), (newer, "0.1.0"), ("0.1.0", "0.1.1-main.20270101000000")]:
                da, db = vf.forms(a)["deb"], vf.forms(b)["deb"]
                ok = subprocess.run(["dpkg", "--compare-versions", da, "lt", db]).returncode == 0
                self.assertTrue(ok, f"deb: {da} isn't before {db}")
        if shutil.which("rpm"):
            def evr(v):
                f = vf.forms(v)
                return f"{f['rpm_version']}-{f['rpm_release']}"
            for a, b in [(older, newer), (newer, "0.1.0")]:
                out = subprocess.run(["rpm", "--eval", f"%{{lua: print(rpm.vercmp('{evr(a)}', '{evr(b)}'))}}"], capture_output=True, text=True).stdout.strip()
                self.assertEqual(out, "-1", f"rpm: {evr(a)} isn't before {evr(b)}")

    def test_rewrites_the_deb_control_version(self):
        norm = vf._normalizer()
        work = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, work)
        control = f"Package: lockdown\nVersion: {V1}\nArchitecture: amd64\n".encode()

        def tgz(files):
            buf = io.BytesIO()
            with tarfile.open(fileobj=buf, mode="w") as t:
                for name, blob in files:
                    info = tarfile.TarInfo(name)
                    info.size = len(blob)
                    t.addfile(info, io.BytesIO(blob))
            return norm.gzip_bytes(buf.getvalue())

        deb = work / "lockdown.deb"
        deb.write_bytes(norm.write_ar([("debian-binary", b"2.0\n"), ("control.tar.gz", tgz([("./control", control), ("./md5sums", b"")])), ("data.tar.gz", tgz([("./usr/bin/x", b"x")]))], 0))
        self.assertEqual(vf.rewrite_deb(deb, V1), "0.1.0~main.20261107093000")
        members = dict(norm.read_ar(deb.read_bytes()))
        with tarfile.open(fileobj=io.BytesIO(members["control.tar.gz"]), mode="r:gz") as t:
            text = t.extractfile("./control").read().decode()
        self.assertIn("Version: 0.1.0~main.20261107093000\n", text)
        self.assertIn("Package: lockdown\n", text)
        self.assertEqual(members["data.tar.gz"], tgz([("./usr/bin/x", b"x")]))
        if shutil.which("dpkg-deb"):
            out = subprocess.run(["dpkg-deb", "-f", str(deb), "Version"], capture_output=True, text=True).stdout.strip()
            self.assertEqual(out, "0.1.0~main.20261107093000")


class VersionStamp(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        for rel in ["Cargo.toml", "Cargo.lock", "apps/desktop/src-tauri/tauri.conf.json", "apps/desktop/package.json", "apps/web/package.json"]:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / rel, self.root / rel)

    def test_every_manifest_gets_the_same_version(self):
        sv.stamp(self.root, V1)
        cargo = (self.root / "Cargo.toml").read_text()
        self.assertIn(f'version = "{V1}"', cargo)
        for rel in ["apps/desktop/src-tauri/tauri.conf.json", "apps/desktop/package.json", "apps/web/package.json"]:
            self.assertEqual(json.loads((self.root / rel).read_text())["version"], V1, rel)
        lock = (self.root / "Cargo.lock").read_text()
        self.assertIn(f'name = "lockdown-desktop"\nversion = "{V1}"', lock)
        self.assertIn(f'name = "lockdown-app"\nversion = "{V1}"', lock)
        # Crates from the registry keep their own versions.
        self.assertNotIn(f'version = "{V1}"\nsource =', lock)
        # Only version lines changed.
        before = (ROOT / "apps/desktop/src-tauri/tauri.conf.json").read_text().splitlines()
        after = (self.root / "apps/desktop/src-tauri/tauri.conf.json").read_text().splitlines()
        self.assertEqual([a for a, b in zip(before, after) if a != b], ['  "version": "0.1.0",'])

    def test_refuses_another_scheme_or_a_second_stamp(self):
        for bad in ["0.1.0", "0.2.0-main.20261107093000", "0.1.0-main.1"]:
            with self.assertRaises(SystemExit, msg=bad):
                sv.stamp(self.root, bad)
        sv.stamp(self.root, V1)
        with self.assertRaises(SystemExit):
            sv.stamp(self.root, V2)

    def test_the_stamped_workspace_still_resolves_locked(self):
        if shutil.which("cargo") is None:
            self.skipTest("cargo isn't installed here")
        work = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, work)
        subprocess.run(["git", "-C", str(ROOT), "worktree", "list"], check=True, capture_output=True)
        # A copy of the tracked tree, stamped, must still satisfy --locked.
        files = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z"], check=True, capture_output=True).stdout.split(b"\0")
        for f in files:
            if not f:
                continue
            src = ROOT / f.decode()
            if src.is_file():
                dst = work / f.decode()
                dst.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(src, dst)
        sv.stamp(work, V1)
        r = subprocess.run(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=work, capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        versions = {p["name"]: p["version"] for p in json.loads(r.stdout)["packages"]}
        self.assertEqual(versions["lockdown-desktop"], V1)


if __name__ == "__main__":
    unittest.main()
