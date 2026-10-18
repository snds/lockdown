//! Regenerates the signed activation-page allow-list from its source with
//! the development key (docs/plan/26-desktop-finish.md §3).
//!
//! ```text
//! cargo run -p lockdown-approval --example sign-activation-pages            # write
//! cargo run -p lockdown-approval --example sign-activation-pages -- --check # exit 1 if stale
//! ```
//!
//! Reads `data/activation-pages.json` (edited by hand) and the matcher's
//! development seed; writes `data/activation-pages.signed.cbor`. Release
//! files are signed by the release signing service with a key that is not
//! in this repository (`crates/matcher/data/dev/README.md`, D63).

use std::path::PathBuf;
use std::process::ExitCode;

use ed25519_dalek::SigningKey;
use lockdown_approval::activation::{ActivationPayload, sign, verify};
use lockdown_matcher::signed::{DEV_KEY_ID, TrustedKeys};

fn fail(why: String) -> ExitCode {
    eprintln!("sign-activation-pages: {why}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let read =
        |p: PathBuf| std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()));
    let payload: ActivationPayload = match read(root.join("data/activation-pages.json"))
        .and_then(|s| serde_json::from_str(&s).map_err(|e| format!("the source isn't valid: {e}")))
    {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let seed_path = root.join("../matcher/data/dev/publisher.key");
    let seed = match read(seed_path.clone()).and_then(|hex| {
        data_encoding::HEXLOWER
            .decode(hex.trim().as_bytes())
            .map_err(|e| format!("{} isn't lowercase hex: {e}", seed_path.display()))
    }) {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    let Ok(seed) = <[u8; 32]>::try_from(seed) else {
        return fail(format!("{} isn't 32 bytes", seed_path.display()));
    };
    let bytes = match sign(&payload, &SigningKey::from_bytes(&seed), DEV_KEY_ID) {
        Ok(b) => b,
        Err(e) => return fail(format!("sign: {e}")),
    };
    if let Err(e) = verify(&bytes, &TrustedKeys::dev()) {
        return fail(format!("the fresh file doesn't verify: {e}"));
    }
    let out = root.join("data/activation-pages.signed.cbor");
    if check {
        if std::fs::read(&out).unwrap_or_default() != bytes {
            return fail(format!("{} is stale; run without --check", out.display()));
        }
        println!(
            "activation pages are current (version {})",
            payload.activation_pages
        );
        return ExitCode::SUCCESS;
    }
    if let Err(e) = std::fs::write(&out, &bytes) {
        return fail(format!("write {}: {e}", out.display()));
    }
    println!(
        "wrote {} ({} bytes, version {}, {} pages)",
        out.display(),
        bytes.len(),
        payload.activation_pages,
        payload.pages.len()
    );
    ExitCode::SUCCESS
}
