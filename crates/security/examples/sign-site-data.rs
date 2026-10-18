//! Regenerates the security center's signed site data from its sources
//! with the development key (docs/plan/23-bridges.md §3).
//!
//! ```text
//! cargo run -p lockdown-security --example sign-site-data            # write
//! cargo run -p lockdown-security --example sign-site-data -- --check # exit 1 if stale
//! ```
//!
//! Reads `data/site-data/curated.json` (the version, providers, recipes and
//! fixes, edited by hand), `data/site-data/guides.json` (cut from the two
//! directories by `data/snapshot-guides.py`) and the matcher's development
//! seed; writes `data/site-data.signed.cbor`. Release files are signed by
//! the release signing service with a key that is not in this repository
//! (`crates/matcher/data/dev/README.md`).

use std::path::PathBuf;
use std::process::ExitCode;

use ed25519_dalek::SigningKey;
use lockdown_matcher::signed::{DEV_KEY_ID, TrustedKeys};
use lockdown_security::site_data::{payload_from_sources, sign, verify};

fn fail(why: String) -> ExitCode {
    eprintln!("sign-site-data: {why}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let read =
        |p: PathBuf| std::fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()));
    let (curated, guides) = match (
        read(root.join("data/site-data/curated.json")),
        read(root.join("data/site-data/guides.json")),
    ) {
        (Ok(c), Ok(g)) => (c, g),
        (Err(e), _) | (_, Err(e)) => return fail(e),
    };
    let payload = match payload_from_sources(&curated, &guides) {
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
    let key = SigningKey::from_bytes(&seed);
    let bytes = match sign(&payload, &key, DEV_KEY_ID) {
        Ok(b) => b,
        Err(e) => return fail(format!("sign: {e}")),
    };
    if let Err(e) = verify(&bytes, &TrustedKeys::dev()) {
        return fail(format!("the fresh file doesn't verify: {e}"));
    }
    let out = root.join("data/site-data.signed.cbor");
    if check {
        let current = std::fs::read(&out).unwrap_or_default();
        if current != bytes {
            return fail(format!("{} is stale; run without --check", out.display()));
        }
        println!("site data is current (version {})", payload.site_data);
        return ExitCode::SUCCESS;
    }
    if let Err(e) = std::fs::write(&out, &bytes) {
        return fail(format!("write {}: {e}", out.display()));
    }
    println!(
        "wrote {} ({} bytes, version {}, {} guides, {} providers, {} recipes, {} fixes)",
        out.display(),
        bytes.len(),
        payload.site_data,
        payload.guides.len(),
        payload.providers.len(),
        payload.recipes.len(),
        payload.fixes.len()
    );
    ExitCode::SUCCESS
}
