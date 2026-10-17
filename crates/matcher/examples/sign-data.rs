//! Regenerates the signed matcher data file from its JSON source with the
//! development key.
//!
//! ```text
//! cargo run -p lockdown-matcher --example sign-data            # write
//! cargo run -p lockdown-matcher --example sign-data -- --check # exit 1 if stale
//! ```
//!
//! Reads `data/matcher-data.json` and `data/dev/publisher.key`; writes
//! `data/dev/publisher.pub` and `data/matcher-data.signed.cbor`. Production
//! files are signed by the release signing service with a key that is not
//! in this repository (`data/dev/README.md`).

use std::path::PathBuf;
use std::process::ExitCode;

use ed25519_dalek::SigningKey;
use lockdown_matcher::signed::{DEV_KEY_ID, MatcherPayload, TrustedKeys, sign, verify};

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("data/matcher-data.json");
    let seed_path = root.join("data/dev/publisher.key");
    let pub_path = root.join("data/dev/publisher.pub");
    let out = root.join("data/matcher-data.signed.cbor");

    let json = match std::fs::read_to_string(&source) {
        Ok(s) => s,
        Err(e) => return fail(format!("read {}: {e}", source.display())),
    };
    let payload: MatcherPayload = match serde_json::from_str(&json) {
        Ok(p) => p,
        Err(e) => {
            return fail(format!(
                "{} doesn't parse as the payload: {e}",
                source.display()
            ));
        }
    };
    let seed_hex = match std::fs::read_to_string(&seed_path) {
        Ok(s) => s,
        Err(e) => return fail(format!("read {}: {e}", seed_path.display())),
    };
    let seed = match data_encoding::HEXLOWER.decode(seed_hex.trim().as_bytes()) {
        Ok(b) => b,
        Err(e) => return fail(format!("{} isn't lowercase hex: {e}", seed_path.display())),
    };
    let seed: [u8; 32] = match seed.try_into() {
        Ok(s) => s,
        Err(_) => return fail(format!("{} isn't 32 bytes", seed_path.display())),
    };
    let key = SigningKey::from_bytes(&seed);
    let pub_hex = data_encoding::HEXLOWER.encode(&key.verifying_key().to_bytes());

    let bytes = match sign(&payload, &key, DEV_KEY_ID) {
        Ok(b) => b,
        Err(e) => return fail(format!("sign: {e}")),
    };
    let keys = match TrustedKeys::none().with_hex(DEV_KEY_ID, &pub_hex) {
        Ok(k) => k,
        Err(e) => return fail(format!("key: {e}")),
    };
    if let Err(e) = verify(&bytes, &keys) {
        return fail(format!("the freshly signed file doesn't verify: {e}"));
    }

    let pub_on_disk = std::fs::read_to_string(&pub_path).unwrap_or_default();
    let signed_on_disk = std::fs::read(&out).unwrap_or_default();
    let pub_stale = pub_on_disk.trim() != pub_hex;
    let signed_stale = signed_on_disk != bytes;

    if check {
        if pub_stale || signed_stale {
            eprintln!(
                "stale: publisher.pub {} · matcher-data.signed.cbor {}",
                if pub_stale { "differs" } else { "ok" },
                if signed_stale { "differs" } else { "ok" }
            );
            return ExitCode::from(1);
        }
        println!(
            "up to date: matcher_data {} signed by {} ({} bytes)",
            payload.matcher_data,
            DEV_KEY_ID,
            bytes.len()
        );
        return ExitCode::SUCCESS;
    }
    if pub_stale {
        if let Err(e) = std::fs::write(&pub_path, format!("{pub_hex}\n")) {
            return fail(format!("write {}: {e}", pub_path.display()));
        }
        println!("wrote {}", pub_path.display());
    }
    if signed_stale {
        if let Err(e) = std::fs::write(&out, &bytes) {
            return fail(format!("write {}: {e}", out.display()));
        }
        println!(
            "wrote {} ({} bytes, matcher_data {}, key {})",
            out.display(),
            bytes.len(),
            payload.matcher_data,
            DEV_KEY_ID
        );
    } else {
        println!("matcher-data.signed.cbor already up to date");
    }
    ExitCode::SUCCESS
}

fn fail(msg: String) -> ExitCode {
    eprintln!("sign-data: {msg}");
    ExitCode::from(2)
}
