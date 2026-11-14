#!/usr/bin/env python3
"""Tests for the main channel's scripts (docs/plan/60-auto-update.md §7):
the manifest writer and the version stamp; and for the shared feed's
(docs/plan/66-shared-feed.md): the same manifest for a second address, the
server feed's manifest, and the publish script against a stand-in `gh`.
Run from the repository root:

    python3 packaging/update/test_update_scripts.py
"""

import base64
import importlib.util
import io
import json
import os
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
wsm = load("write-server-manifest")

V1 = "0.1.0-main.20261107093000"
V2 = "0.1.0-main.20261107120000"
BASE = "https://github.com/snds/lockdown/releases/download/main-build"
# The shared feed's address (docs/plan/66-shared-feed.md §1).
SHARED = "https://github.com/snds/releases/releases/download/lockdown-main"


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


class SharedFeed(ManifestWriter):
    """The same build written for both addresses (docs/plan/66-shared-feed.md §2)."""

    # Only this class's own tests: the writer's are run by `ManifestWriter`.
    def test_the_two_manifests_differ_only_in_where_the_files_are(self):
        digest = "sha256:" + "ab" * 32
        bundles = self.all_three()
        bridge = wm.write(V1, "2026-11-07T09:30:00Z", BASE, bundles, "notes", server_image_digest=digest)
        shared = wm.write(V1, "2026-11-07T09:30:00Z", SHARED, bundles, "notes", server_image_digest=digest)
        self.assertEqual(sorted(bridge), sorted(shared))
        for key in ("version", "notes", "pub_date", "server_image"):
            self.assertEqual(bridge[key], shared[key], key)
        self.assertEqual(sorted(bridge["platforms"]), sorted(shared["platforms"]))
        for key, old in bridge["platforms"].items():
            new = shared["platforms"][key]
            self.assertEqual(old["signature"], new["signature"], "the same signed file")
            self.assertTrue(old["url"].startswith(BASE + "/"), old["url"])
            self.assertTrue(new["url"].startswith(SHARED + "/"), new["url"])
            self.assertEqual(old["url"][len(BASE):], new["url"][len(SHARED):], "the same file name")

    def test_each_feed_has_its_own_never_backwards_check(self):
        bundles = self.all_three(V2)
        # The bridge is behind and the shared feed is ahead (or the other way): each is judged alone.
        self.assertEqual(wm.write(V2, "", BASE, bundles, "", previous={"version": V1})["version"], V2)
        with self.assertRaisesRegex(wm.Refused, "isn't newer"):
            wm.write(V2, "", SHARED, bundles, "", previous={"version": V2})
        # The shared feed starts empty.
        self.assertEqual(wm.write(V2, "", SHARED, bundles, "", previous=None)["version"], V2)


for _name in [n for n in dir(ManifestWriter) if n.startswith("test_")]:
    setattr(SharedFeed, _name, None)


COMMIT = "0123456789abcdef0123456789abcdef01234567"
DIGEST = "sha256:" + "ab" * 32
IMAGE = f"ghcr.io/snds/lockdown-server@{DIGEST}"


class ServerManifest(unittest.TestCase):
    """The server feed's manifest (docs/plan/66-shared-feed.md §3)."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)

    def test_writes_the_shape_every_server_feed_has(self):
        self.assertEqual(
            wsm.write(V1, COMMIT, IMAGE),
            {
                "app": "lockdown-server",
                "version": V1,
                "commit": COMMIT,
                "pubDate": "2026-11-07T09:30:00Z",
                "images": {"server": {"repository": "ghcr.io/snds/lockdown-server", "digest": DIGEST}},
            },
        )

    def test_names_the_image_the_server_runs(self):
        # The repository in the manifest is the one the server pulls (bins/server/src/updates.rs).
        source = (ROOT / "bins/server/src/updates.rs").read_text()
        self.assertIn(f'pub const IMAGE: &str = "{wsm.REPOSITORY}";', source)

    def test_refuses_wrong_inputs(self):
        for version in ["0.1.0", "0.1.0-main", "0.1.0-main.2026", "0.1.0-beta.1", ""]:
            with self.assertRaises(wsm.Refused, msg=version) as c:
                wsm.write(version, COMMIT, IMAGE)
            self.assertEqual(c.exception.code, wsm.BAD)
        for commit in ["", "0123", COMMIT.upper(), COMMIT + "0"]:
            with self.assertRaises(wsm.Refused, msg=commit):
                wsm.write(V1, commit, IMAGE)
        for image in [
            "ghcr.io/snds/lockdown-server",
            "ghcr.io/snds/lockdown-server:main",
            "ghcr.io/snds/lockdown-server@sha256:abc",
            "ghcr.io/snds/lockdown-server@sha512:" + "ab" * 32,
            "ghcr.io/snds/lockdown-server@sha256:" + "AB" * 32,
            f"ghcr.io/someone/lockdown-server@{DIGEST}",
            f"docker.io/snds/lockdown-server@{DIGEST}",
        ]:
            with self.assertRaises(wsm.Refused, msg=image):
                wsm.write(V1, COMMIT, image)

    def test_never_takes_the_feed_backwards(self):
        published = lambda v: json.dumps({"version": v})
        self.assertEqual(wsm.write(V2, COMMIT, IMAGE, published(V1))["version"], V2)
        with self.assertRaises(wsm.Refused) as c:
            wsm.write(V1, COMMIT, IMAGE, published(V1))
        self.assertEqual(c.exception.code, wsm.ALREADY)
        with self.assertRaises(wsm.Refused) as c:
            wsm.write(V1, COMMIT, IMAGE, published(V2))
        self.assertEqual(c.exception.code, wsm.NEWER_PUBLISHED)
        # A tagged release sorts after its own main builds, as semver has it.
        with self.assertRaises(wsm.Refused) as c:
            wsm.write(V1, COMMIT, IMAGE, published("0.1.0"))
        self.assertEqual(c.exception.code, wsm.NEWER_PUBLISHED)

    def test_never_writes_over_a_manifest_it_can_not_read(self):
        for text in ["", "<html>", "[]", "{}", '{"version": 3}', '{"version": "next"}']:
            with self.assertRaises(wsm.Refused, msg=text) as c:
                wsm.write(V1, COMMIT, IMAGE, text)
            self.assertEqual(c.exception.code, wsm.BAD)

    def test_the_command_line_writes_only_on_success(self):
        out = self.dir / "latest.json"
        args = ["--version", V1, "--commit", COMMIT, "--image", IMAGE, "--out", str(out)]
        self.assertEqual(wsm.main(args), 0)
        self.assertEqual(json.loads(out.read_text())["images"]["server"]["digest"], DIGEST)
        out.unlink()
        previous = self.dir / "published.json"
        previous.write_text(json.dumps({"version": V1}))
        self.assertEqual(wsm.main(args + ["--previous", str(previous)]), wsm.ALREADY)
        previous.write_text(json.dumps({"version": V2}))
        self.assertEqual(wsm.main(args + ["--previous", str(previous)]), wsm.NEWER_PUBLISHED)
        previous.write_text("not json")
        self.assertEqual(wsm.main(args + ["--previous", str(previous)]), wsm.BAD)
        self.assertFalse(out.exists(), "a refusal writes nothing")
        # No file at that path: nothing is published yet.
        self.assertEqual(wsm.main(args + ["--previous", str(self.dir / "absent.json")]), 0)


# A stand-in for `gh`: releases are folders under $FAKE_GH, and every call is
# one line of $FAKE_GH/log. It knows only the calls publish-feed.sh makes,
# and fails on any other, so a changed script fails here first.
FAKE_GH = r"""#!/usr/bin/env python3
import json, os, shutil, sys
from pathlib import Path

state = Path(os.environ["FAKE_GH"])
args = sys.argv[1:]
with (state / "log").open("a") as log:
    log.write(" ".join(args) + "\n")

def flag(name, takes=1):
    if name not in args:
        return None
    i = args.index(name)
    value = args[i + 1] if takes else True
    del args[i:i + 1 + takes]
    return value

def release(repo, tag):
    return state / repo.replace("/", "__") / tag

def names(folder):
    return sorted(p.name for p in folder.iterdir() if p.name != ".release")

if args[:1] == ["api"]:
    sys.exit(1 if (state / "unreachable").exists() else 0)
if args[:1] != ["release"]:
    sys.exit(f"fake gh: unknown call {args}")
verb, repo = args[1], flag("--repo")
rest = args[2:]
if verb == "view":
    fields, query = flag("--json"), flag("-q")
    rest = args[2:]
    folder = release(repo, rest[0])
    if not folder.is_dir():
        sys.exit(1)
    meta = json.loads((folder / ".release").read_text())
    if fields == "assets":
        print("\n".join(names(folder)))
    elif fields:
        print(json.dumps({"prerelease": meta["prerelease"], "tag": rest[0], "files": names(folder)}))
elif verb in ("create", "edit"):
    pre, latest = flag("--prerelease", 0), "--latest=false" in args
    title, notes = flag("--title"), flag("--notes")
    rest = [a for a in args[2:] if a != "--latest=false"]
    folder = release(repo, rest[0])
    if (verb == "create") == folder.is_dir():
        sys.exit(f"fake gh: {verb} on a release that {'exists' if folder.is_dir() else 'is missing'}")
    folder.mkdir(parents=True, exist_ok=True)
    (folder / ".release").write_text(json.dumps({"prerelease": bool(pre), "not_latest": latest, "title": title, "notes": notes}))
elif verb == "upload":
    if not flag("--clobber", 0):
        sys.exit("fake gh: upload without --clobber")
    rest = args[2:]
    folder = release(repo, rest[0])
    if (state / "upload-fails").exists():
        sys.exit(1)
    for f in rest[1:]:
        shutil.copy(f, folder / Path(f).name)
elif verb == "delete-asset":
    flag("--yes", 0)
    rest = args[2:]
    (release(repo, rest[0]) / rest[1]).unlink()
elif verb == "download":
    pattern, out = flag("--pattern"), flag("--output")
    rest = args[2:]
    if (state / "download-fails").exists():
        sys.exit(1)
    shutil.copy(release(repo, rest[0]) / pattern, out)
else:
    sys.exit(f"fake gh: unknown call {args}")
"""

REPO = "snds/releases"
TAG = "lockdown-main"


@unittest.skipUnless(shutil.which("bash") and os.name == "posix", "publish-feed.sh is a bash script")
class PublishFeed(unittest.TestCase):
    """packaging/update/publish-feed.sh against a stand-in `gh` (docs/plan/66-shared-feed.md §2)."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.state = self.dir / "state"
        self.state.mkdir()
        tools = self.dir / "bin"
        tools.mkdir()
        gh = tools / "gh"
        gh.write_text(FAKE_GH)
        gh.chmod(0o755)
        self.env = {**os.environ, "PATH": f"{tools}{os.pathsep}{os.environ['PATH']}", "FAKE_GH": str(self.state),
                    "GH_TOKEN": "not-a-token"}
        self.release = self.state / "snds__releases" / TAG

    def run_script(self, *args):
        return subprocess.run(["bash", str(HERE / "publish-feed.sh"), *args], env=self.env, capture_output=True, text=True)

    def log(self):
        log = self.state / "log"
        return log.read_text().splitlines() if log.exists() else []

    def feed(self, version, files):
        d = self.dir / f"feed-{version}"
        d.mkdir()
        for name in files:
            (d / name).write_text(f"{name} of {version}")
        (d / "latest.json").write_text(json.dumps({"version": version}))
        return d

    def publish(self, d):
        return self.run_script("publish", REPO, TAG, "Lockdown main", "notes", str(d))

    def test_the_first_run_makes_a_pre_release_that_is_never_latest(self):
        r = self.publish(self.feed(V1, ["a.tar.gz", "a.tar.gz.sig"]))
        self.assertEqual(r.returncode, 0, r.stderr)
        meta = json.loads((self.release / ".release").read_text())
        self.assertEqual(meta, {"prerelease": True, "not_latest": True, "title": "Lockdown main", "notes": "notes"})
        self.assertEqual(sorted(p.name for p in self.release.iterdir()), [".release", "a.tar.gz", "a.tar.gz.sig", "latest.json"])

    def test_files_go_up_first_and_the_manifest_last_then_old_files_go(self):
        self.assertEqual(self.publish(self.feed(V1, ["old.tar.gz", "old.tar.gz.sig"])).returncode, 0)
        (self.state / "log").unlink()
        r = self.publish(self.feed(V2, ["new.tar.gz", "new.tar.gz.sig"]))
        self.assertEqual(r.returncode, 0, r.stderr)
        calls = [c for c in self.log() if c.startswith(("release upload", "release delete-asset", "release edit", "release create"))]
        self.assertTrue(calls[0].startswith("release edit"), calls)
        self.assertIn("new.tar.gz", calls[1])
        self.assertNotIn("latest.json", calls[1])
        self.assertTrue(calls[2].endswith("latest.json"), calls)
        self.assertEqual(sorted(c.split()[3] for c in calls[3:]), ["old.tar.gz", "old.tar.gz.sig"])
        self.assertTrue(all(c.startswith("release delete-asset") for c in calls[3:]), calls)
        self.assertEqual(json.loads((self.release / "latest.json").read_text())["version"], V2)
        self.assertEqual(sorted(p.name for p in self.release.iterdir()), [".release", "latest.json", "new.tar.gz", "new.tar.gz.sig"])

    def test_a_failed_upload_leaves_the_old_manifest_and_removes_nothing(self):
        self.assertEqual(self.publish(self.feed(V1, ["old.tar.gz"])).returncode, 0)
        (self.state / "upload-fails").touch()
        r = self.publish(self.feed(V2, ["new.tar.gz"]))
        self.assertNotEqual(r.returncode, 0)
        self.assertEqual(json.loads((self.release / "latest.json").read_text())["version"], V1)
        self.assertTrue((self.release / "old.tar.gz").exists())
        self.assertFalse(any(c.startswith("release delete-asset") for c in self.log()))

    def test_a_manifest_alone_is_a_whole_feed(self):
        # The server feed: latest.json and nothing else.
        r = self.publish(self.feed(V1, []))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(sorted(p.name for p in self.release.iterdir()), [".release", "latest.json"])

    def test_refuses_a_folder_without_a_manifest_and_a_token_that_can_not_read(self):
        empty = self.dir / "empty"
        empty.mkdir()
        self.assertNotEqual(self.publish(empty).returncode, 0)
        self.assertEqual(self.log(), [], "nothing was asked of GitHub")
        (self.state / "unreachable").touch()
        r = self.publish(self.feed(V1, ["a.tar.gz"]))
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("can't read snds/releases", r.stdout)
        self.assertFalse(self.release.exists())

    def test_previous_reads_what_is_published(self):
        out = self.dir / "published.json"
        # No release yet, then a release with no manifest yet: nothing to compare with, and no failure.
        r = self.run_script("previous", REPO, TAG, str(out))
        self.assertEqual((r.returncode, out.exists()), (0, False), r.stderr)
        self.release.mkdir(parents=True)
        (self.release / ".release").write_text("{}")
        r = self.run_script("previous", REPO, TAG, str(out))
        self.assertEqual((r.returncode, out.exists()), (0, False), r.stderr)
        (self.release / "latest.json").write_text(json.dumps({"version": V1}))
        r = self.run_script("previous", REPO, TAG, str(out))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(json.loads(out.read_text())["version"], V1)

    def test_previous_fails_when_a_published_manifest_can_not_be_read(self):
        out = self.dir / "published.json"
        out.write_text("left over from another run")
        self.release.mkdir(parents=True)
        (self.release / ".release").write_text("{}")
        (self.release / "latest.json").write_text(json.dumps({"version": V2}))
        (self.state / "download-fails").touch()
        r = self.run_script("previous", REPO, TAG, str(out))
        self.assertNotEqual(r.returncode, 0)
        self.assertFalse(out.exists(), "a stale file is never taken for the published one")
        (self.state / "download-fails").unlink()
        (self.state / "unreachable").touch()
        self.assertNotEqual(self.run_script("previous", REPO, TAG, str(out)).returncode, 0)

    def test_the_token_is_never_an_argument(self):
        self.assertEqual(self.publish(self.feed(V1, ["a.tar.gz"])).returncode, 0)
        self.assertNotIn("not-a-token", "\n".join(self.log()))


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
