//! An install the way Lockdown made one before every vault had a master
//! password (docs/plan/43-cli-sealed.md): the device key in the plain file,
//! the identity and the persona's first vault, no account and no kit.
//!
//! Nothing that ships can make one any more. `lockdown init` seals every
//! vault it makes; installs made earlier keep opening, and the tests and
//! the web vault's end-to-end harness that check that need one to start
//! from. This is how they get it: the app's own core, opened with `create`
//! and no master password, the way the desktop's "This device only" made
//! it. Behind the `legacy-install` feature, which only the command line's
//! tests and the `lockdown-legacy-install` tool turn on; anything that links
//! it carries [`crate::MARKER`], so the release check would find it.

use std::path::{Path, PathBuf};

/// Makes a passwordless install at `root` (and `data`, else the `lockdown`
/// folder beside the root) and returns the data directory. Refuses a place
/// that holds anything already.
pub fn make(root: &Path, data: Option<&Path>) -> Result<PathBuf, String> {
    let paths =
        lockdown_app::Paths::resolve(Some(root.to_path_buf()), data.map(Path::to_path_buf))?;
    if paths.root.exists() || paths.data.exists() {
        return Err(format!(
            "{} or {} exists already; a passwordless install is made only on nothing ({})",
            paths.root.display(),
            paths.data.display(),
            crate::MARKER
        ));
    }
    let data = paths.data.clone();
    let app = lockdown_app::App::start(paths, None, lockdown_app::LockPolicy::default());
    app.open_or_create(true)
        .map_err(|e| format!("{} ({})", e.message, crate::MARKER))?;
    app.lock().map_err(|e| e.message)?;
    Ok(data)
}
