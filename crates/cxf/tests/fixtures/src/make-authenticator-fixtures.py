#!/usr/bin/env python3
"""Writes the authenticator and Steam sample files the importer tests read
(docs/plan/23-bridges.md §1-§2).

Usage: python3 -I make-authenticator-fixtures.py <crates/cxf/tests/fixtures>

Needs `cryptography` (AES-GCM, AES-CBC, scrypt, PBKDF2) and `PyNaCl`
(libsodium's Argon2id and secretstream, which Ente Auth uses). Both are
independent of the Rust code that reads the files, so a test passing
means Lockdown reads what those libraries write, not only what it writes
itself. Every value is made up; the code secret is RFC 6238's test key
("12345678901234567890", base32 GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ). The
shapes follow each app's documented or open-source export format; none
was exported from the apps themselves.
"""

import base64
import json
import os
import sys
import zipfile

import nacl.bindings as sodium
import nacl.pwhash
from cryptography.hazmat.primitives import hashes, padding
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC
from cryptography.hazmat.primitives.kdf.scrypt import Scrypt

KEY = b"12345678901234567890"
B32 = base64.b32encode(KEY).decode()
# Fixed bytes so the files come out the same every run.
SALT = bytes(range(32))
NONCE12 = bytes(range(12))
IV16 = bytes(range(16))


def write(out, name, data):
    mode = "wb" if isinstance(data, bytes) else "w"
    with open(os.path.join(out, name), mode) as f:
        f.write(data)


# --- Google Authenticator -------------------------------------------------


def varint(v):
    out = bytearray()
    while True:
        b = v & 0x7F
        v >>= 7
        if v:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def field_bytes(n, v):
    return varint(n << 3 | 2) + varint(len(v)) + v


def field_num(n, v):
    return varint(n << 3) + varint(v)


def otp_parameters(secret, name, issuer, algo, digits, kind, counter=0):
    return (
        field_bytes(1, secret)
        + field_bytes(2, name.encode())
        + field_bytes(3, issuer.encode())
        + field_num(4, algo)
        + field_num(5, digits)
        + field_num(6, kind)
        + (field_num(7, counter) if counter else b"")
    )


def migration(params, index, size, batch_id):
    payload = b"".join(field_bytes(1, p) for p in params)
    payload += field_num(2, 1) + field_num(3, size) + field_num(4, index) + field_num(5, batch_id)
    from urllib.parse import quote

    return "otpauth-migration://offline?data=" + quote(base64.b64encode(payload).decode(), safe="")


def google(out):
    first = migration(
        [
            otp_parameters(KEY, "Example:sean@example.com", "Example", 1, 1, 2),
            otp_parameters(KEY, "Bank:sean", "Bank", 2, 2, 2),
        ],
        0,
        2,
        777,
    )
    second = migration([otp_parameters(KEY, "Counter:hotp@example.com", "Counter", 1, 1, 1, 5)], 1, 2, 777)
    write(out, "google-authenticator.txt", first + "\n" + second + "\n")


# --- Aegis ------------------------------------------------------------------


def aegis_db():
    return {
        "version": 3,
        "entries": [
            {
                "type": "totp",
                "uuid": "0b1c6f7e-0000-4000-8000-000000000001",
                "name": "sean@example.com",
                "issuer": "Example",
                "note": "work account",
                "favorite": True,
                "icon": None,
                "info": {"secret": B32, "algo": "SHA1", "digits": 6, "period": 30},
                "groups": ["7a9d1b42-0000-4000-8000-0000000000aa"],
            },
            {
                "type": "hotp",
                "uuid": "0b1c6f7e-0000-4000-8000-000000000002",
                "name": "counter",
                "issuer": "Counter",
                "note": "",
                "favorite": False,
                "info": {"secret": B32, "algo": "SHA256", "digits": 8, "counter": 3},
                "groups": [],
            },
            {
                "type": "steam",
                "uuid": "0b1c6f7e-0000-4000-8000-000000000003",
                "name": "gaben",
                "issuer": "Steam",
                "note": "",
                "favorite": False,
                "info": {"secret": B32, "algo": "SHA1", "digits": 5, "period": 30},
                "groups": [],
            },
            {
                "type": "motp",
                "uuid": "0b1c6f7e-0000-4000-8000-000000000004",
                "name": "old",
                "issuer": "Mobile-OTP",
                "note": "",
                "favorite": False,
                "info": {"secret": "ABCDEF", "algo": "MD5", "digits": 6, "period": 10, "pin": "1234"},
                "groups": [],
            },
        ],
        "groups": [{"uuid": "7a9d1b42-0000-4000-8000-0000000000aa", "name": "Work"}],
    }


def aegis(out):
    plain = {"version": 1, "header": {"slots": None, "params": None}, "db": aegis_db()}
    write(out, "aegis-plain.json", json.dumps(plain, indent=2))
    password = b"fixture-aegis"
    master = bytes(range(100, 132))
    n, r, p = 1 << 10, 8, 1
    derived = Scrypt(salt=SALT, length=32, n=n, r=r, p=p).derive(password)
    slot_nonce = bytes(range(50, 62))
    sealed_key = AESGCM(derived).encrypt(slot_nonce, master, None)
    db_nonce = bytes(range(70, 82))
    sealed_db = AESGCM(master).encrypt(db_nonce, json.dumps(aegis_db()).encode(), None)
    encrypted = {
        "version": 1,
        "header": {
            "slots": [
                {
                    "type": 2,
                    "uuid": "slot-biometric",
                    "key": "00" * 32,
                    "key_params": {"nonce": "00" * 12, "tag": "00" * 16},
                },
                {
                    "type": 1,
                    "uuid": "slot-password",
                    "key": sealed_key[:-16].hex(),
                    "key_params": {"nonce": slot_nonce.hex(), "tag": sealed_key[-16:].hex()},
                    "n": n,
                    "r": r,
                    "p": p,
                    "salt": SALT.hex(),
                    "repaired": True,
                },
            ],
            "params": {"nonce": db_nonce.hex(), "tag": sealed_db[-16:].hex()},
        },
        "db": base64.b64encode(sealed_db[:-16]).decode(),
    }
    write(out, "aegis-encrypted.json", json.dumps(encrypted, indent=2))


# --- 2FAS -------------------------------------------------------------------


def twofas_services():
    return [
        {
            "name": "Example",
            "secret": B32,
            "updatedAt": 1760000000000,
            "otp": {
                "label": "Example:sean@example.com",
                "account": "sean@example.com",
                "issuer": "Example",
                "digits": 6,
                "period": 30,
                "algorithm": "SHA1",
                "tokenType": "TOTP",
                "source": "Link",
            },
            "order": {"position": 0},
            "groupId": "g1",
        },
        {
            "name": "Steam",
            "secret": B32,
            "otp": {"account": "gaben", "issuer": "Steam", "digits": 5, "period": 30, "algorithm": "SHA1", "tokenType": "STEAM"},
            "order": {"position": 1},
        },
    ]


def twofas(out):
    plain = {
        "services": twofas_services(),
        "groups": [{"id": "g1", "name": "Personal", "isExpanded": True}],
        "updatedAt": 1760000000000,
        "schemaVersion": 4,
        "appVersionCode": 5000000,
        "appOrigin": "android",
    }
    write(out, "2fas-plain.2fas", json.dumps(plain, indent=2))
    password = b"fixture-2fas"
    key = PBKDF2HMAC(algorithm=hashes.SHA256(), length=32, salt=SALT, iterations=10000).derive(password)
    sealed = AESGCM(key).encrypt(NONCE12, json.dumps(twofas_services()).encode(), None)
    b64 = lambda b: base64.b64encode(b).decode()
    encrypted = dict(plain)
    encrypted["services"] = []
    encrypted["servicesEncrypted"] = ":".join([b64(sealed), b64(SALT), b64(NONCE12)])
    encrypted["reference"] = b64(AESGCM(key).encrypt(NONCE12, b"reference", None)) + ":" + b64(SALT) + ":" + b64(NONCE12)
    write(out, "2fas-encrypted.2fas", json.dumps(encrypted, indent=2))


# --- Ente Auth ----------------------------------------------------------------


ENTE_TEXT = (
    "otpauth://totp/Example:sean@example.com?secret=" + B32 + "&issuer=Example&algorithm=SHA1&digits=6&period=30\n"
    "otpauth://hotp/Counter:hotp@example.com?secret=" + B32 + "&issuer=Counter&counter=2\n"
)


def ente(out):
    write(out, "ente-plain.txt", ENTE_TEXT)
    password = b"fixture-ente"
    ops, mem = 2, 8 * 1024 * 1024
    key = nacl.pwhash.argon2id.kdf(32, password, SALT[:16], opslimit=ops, memlimit=mem)
    state = sodium.crypto_secretstream_xchacha20poly1305_state()
    header = sodium.crypto_secretstream_xchacha20poly1305_init_push(state, key)
    sealed = sodium.crypto_secretstream_xchacha20poly1305_push(
        state, ENTE_TEXT.encode(), None, sodium.crypto_secretstream_xchacha20poly1305_TAG_FINAL
    )
    b64 = lambda b: base64.b64encode(b).decode()
    encrypted = {
        "version": 1,
        "kdfParams": {"memLimit": mem, "opsLimit": ops, "salt": b64(SALT[:16])},
        "encryptedData": b64(sealed),
        "encryptionNonce": b64(header),
    }
    write(out, "ente-encrypted.json", json.dumps(encrypted, indent=2))


# --- Proton Authenticator and Bitwarden Authenticator -----------------------------


def proton(out):
    doc = {
        "version": 1,
        "entries": [
            {
                "id": "6a1b0f2c-0000-4000-8000-000000000001",
                "content": {
                    "uri": "otpauth://totp/Example:sean@example.com?secret=" + B32 + "&issuer=Example",
                    "entry_type": "Totp",
                    "name": "Example",
                },
                "note": "from Proton",
            },
            {
                "id": "6a1b0f2c-0000-4000-8000-000000000002",
                "content": {"uri": "otpauth://totp/Broken?secret=***", "entry_type": "Totp", "name": "Broken"},
                "note": None,
            },
        ],
    }
    write(out, "proton-authenticator.json", json.dumps(doc, indent=2))


def bitwarden(out):
    doc = {
        "encrypted": False,
        "items": [
            {
                "id": "b1",
                "name": "Example",
                "folderId": None,
                "organizationId": None,
                "collectionIds": None,
                "notes": None,
                "type": 1,
                "login": {"totp": "otpauth://totp/Example:sean@example.com?secret=" + B32 + "&issuer=Example", "username": "sean@example.com"},
                "favorite": True,
            },
            {"id": "b2", "name": "Bare key", "type": 1, "login": {"totp": B32}, "favorite": False},
        ],
    }
    write(out, "bitwarden-authenticator.json", json.dumps(doc, indent=2))
    write(
        out,
        "bitwarden-authenticator.csv",
        "folder,favorite,type,name,login_uri,login_totp\n,1,login,Example,,otpauth://totp/Example:me?secret=" + B32 + "&issuer=Example\n,,login,Bare key,," + B32 + "\n",
    )


# --- Steam --------------------------------------------------------------------


SHARED = base64.b64encode(KEY).decode()


def mafile(name, steamid):
    return {
        "shared_secret": SHARED,
        "serial_number": "17025000000000000000",
        "revocation_code": "R" + steamid[-5:],
        "uri": "otpauth://totp/Steam:" + name + "?secret=GEZDGNBVGY3TQOJQ&issuer=Steam",
        "server_time": 1760000000,
        "account_name": name,
        "token_gid": "2a0000000000000",
        "identity_secret": base64.b64encode(b"identity-" + name.encode()).decode(),
        "secret_1": base64.b64encode(b"secret-1").decode(),
        "status": 1,
        "device_id": "android:00000000-0000-4000-8000-" + steamid[-12:],
        "fully_enrolled": True,
        "Session": {
            "SteamID": int(steamid),
            "AccessToken": "fixture-access-token",
            "RefreshToken": "fixture-refresh-token",
            "SessionID": "fixture-session",
        },
    }


def steam(out):
    write(out, "steam.maFile", json.dumps(mafile("gaben", "76561197960287930")))
    # The Steam app's own JSON (no session), keyed by Steam ID.
    app = {
        "76561197960287931": {
            "steamid": "76561197960287931",
            "shared_secret": SHARED,
            "identity_secret": base64.b64encode(b"identity-app").decode(),
            "revocation_code": "R28793",
            "account_name": "robin",
            "serial_number": "1",
            "uri": "otpauth://totp/Steam:robin?secret=GEZDGNBVGY3TQOJQ&issuer=Steam",
            "token_gid": "1",
            "secret_1": "eA==",
            "server_time": "1760000000",
            "status": 1,
        }
    }
    write(out, "steam-app-v2.json", json.dumps(app, indent=2))
    # SDA's maFiles folder, encrypted with a passkey, as a ZIP.
    passkey = b"fixture-sda"
    accounts = [("gaben", "76561197960287930"), ("robin", "76561197960287931")]
    entries = []
    path = os.path.join(out, "steam-sda-encrypted.zip")
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for i, (name, steamid) in enumerate(accounts):
            salt = bytes([i]) * 8
            iv = bytes([i + 1]) * 16
            key = PBKDF2HMAC(algorithm=hashes.SHA1(), length=32, salt=salt, iterations=50000).derive(passkey)
            padder = padding.PKCS7(128).padder()
            data = padder.update(json.dumps(mafile(name, steamid)).encode()) + padder.finalize()
            enc = Cipher(algorithms.AES(key), modes.CBC(iv)).encryptor()
            body = base64.b64encode(enc.update(data) + enc.finalize()).decode()
            filename = steamid + ".maFile"
            info = zipfile.ZipInfo("maFiles/" + filename, date_time=(2026, 10, 18, 0, 0, 0))
            z.writestr(info, body, zipfile.ZIP_DEFLATED)
            entries.append(
                {
                    "encryption_iv": base64.b64encode(iv).decode(),
                    "encryption_salt": base64.b64encode(salt).decode(),
                    "filename": filename,
                    "steamid": int(steamid),
                }
            )
        manifest = {
            "encrypted": True,
            "first_run": False,
            "entries": entries,
            "periodic_checking": False,
            "periodic_checking_interval": 5,
            "periodic_checking_checkall": False,
            "auto_confirm_market_transactions": False,
            "auto_confirm_trades": False,
        }
        info = zipfile.ZipInfo("maFiles/manifest.json", date_time=(2026, 10, 18, 0, 0, 0))
        z.writestr(info, json.dumps(manifest, indent=2), zipfile.ZIP_DEFLATED)
    write(
        out,
        "winauth.txt",
        "otpauth://totp/Example:sean@example.com?secret=" + B32 + "&digits=6&issuer=Example\n"
        "otpauth://totp/Steam:gaben?secret=" + B32 + "&digits=5&issuer=Steam\n",
    )


def main(out):
    google(out)
    aegis(out)
    twofas(out)
    ente(out)
    proton(out)
    bitwarden(out)
    steam(out)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
