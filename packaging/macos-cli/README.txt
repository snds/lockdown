Lockdown @VERSION@ - the lockdown command for a Mac
====================================================

This folder holds `lockdown`, Lockdown's command line, for every Mac:
Apple silicon and Intel in one file, macOS 11 or later. You don't need
Rust or anything else to run it.

  lockdown     the command
  LICENSE      the Apache License 2.0
  NOTICE       the copyright notice
  README.txt   this file


1. Install it
-------------

Open Terminal in this folder, then pick one place.

Just for you, no administrator password:

  mkdir -p ~/.local/bin
  cp lockdown ~/.local/bin/
  echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc

Then open a new Terminal window. (Skip the last line if ~/.local/bin is
already on your PATH.)

For everyone on this Mac (asks for your password):

  sudo mkdir -p /usr/local/bin
  sudo cp lockdown /usr/local/bin/

Check it:

  lockdown --version

It should print `lockdown @VERSION@`.


2. The first time it runs
-------------------------

A release signed with Lockdown's Developer ID and notarised by Apple just
runs.

A test build is not notarised. It carries only an ad-hoc signature, which
shows the file is intact but not who made it. If you downloaded it with a
web browser, macOS blocks it the first time it runs:

  1. macOS says "lockdown" Not Opened. Choose Done.
  2. Open System Settings, then Privacy & Security.
  3. Scroll down to Security. Next to the line saying "lockdown" was
     blocked, choose Allow Anyway.
  4. Run `lockdown --version` again. If macOS asks once more, choose
     Open Anyway.

If you're comfortable with Terminal, you can instead clear the download
mark on the copy you installed:

  xattr -d com.apple.quarantine ~/.local/bin/lockdown

(or /usr/local/bin/lockdown, with sudo in front).

To see which kind you have:

  codesign -dv --verbose=2 ~/.local/bin/lockdown

A release shows `Authority=Developer ID Application`. A test build shows
`Signature=adhoc`.


3. Set it up
------------

On a Mac with no Lockdown on it yet:

  lockdown init

It asks for your name and a master password (twice, without showing it),
then prints your recovery kit once. Write the kit down or print it, and
keep it somewhere safe. With the kit you can get back in if you forget
the password. Every later command asks for the master password.

To add this Mac to an account you already have, use
`lockdown account login --server URL --login NAME`. With your recovery
kit instead, use `lockdown recover --server URL --login NAME`;
`lockdown recover --help` says how to give it the kit.

If the Lockdown app is already set up on this Mac, the command opens the
same vault with the same master password. The app and the command are one
device.

`lockdown --help` lists every command.


4. Where it keeps your files
----------------------------

The same places as the Lockdown app:

  ~/Library/Application Support/snds/identity   your identity
  ~/Library/Application Support/snds/lockdown   Lockdown's data: the vault
                                                and this Mac's sealed key

Nothing goes under ~/Library/Group Containers. To use other places, give
--root and --data, or set LOCKDOWN_ROOT and LOCKDOWN_DATA.

Removing the command (delete the file you copied) leaves these folders
and your vault alone.


5. Check the download
---------------------

Each release lists every file's SHA-256 in SHA256SUMS, and every file
carries a signed build attestation:

  shasum -a 256 --check --ignore-missing SHA256SUMS
  gh attestation verify lockdown-@VERSION@-macos-universal.tar.gz --repo snds/lockdown

https://github.com/snds/lockdown/blob/main/docs/verify.md says more.
