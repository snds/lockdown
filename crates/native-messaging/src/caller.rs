//! Who started the host. A browser launches a native-messaging host with
//! the caller in its arguments: Chromium passes the extension's origin
//! (`chrome-extension://<id>/`, and on Windows a `--parent-window=` flag
//! after it); Firefox passes the path of the host manifest and the add-on's
//! id. The host reads that and says it first, in a [`HostHello`]; the
//! desktop checks the id against the ids it answers. The browser itself
//! only starts the host for the extensions its manifest lists, so the id
//! is checked twice.

use serde::{Deserialize, Serialize};

use crate::{CHROMIUM_IDS, Error, FIREFOX_IDS, Result};

/// The version of the hello and of the framed JSON after it.
pub const HELLO_V: u32 = 1;

/// Which family of browser, which decides the manifest and the id's form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// Chrome, Chromium, Edge, Brave.
    Chromium,
    /// Firefox.
    Firefox,
}

/// The extension that started the host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caller {
    pub family: Family,
    /// The Chromium extension id or the Firefox add-on id.
    pub extension_id: String,
}

impl Caller {
    /// Reads the caller from the host's arguments (without the program
    /// name).
    pub fn from_args<I, S>(args: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let args: Vec<String> = args.into_iter().map(|a| a.as_ref().to_string()).collect();
        for a in &args {
            if let Some(rest) = a.strip_prefix("chrome-extension://") {
                let id = rest.trim_end_matches('/');
                if is_chromium_id(id) {
                    return Ok(Self {
                        family: Family::Chromium,
                        extension_id: id.to_string(),
                    });
                }
                return Err(Error::UnknownCaller);
            }
        }
        if let [manifest, id, ..] = args.as_slice()
            && manifest.ends_with(".json")
            && !id.is_empty()
            && !id.starts_with('-')
        {
            return Ok(Self {
                family: Family::Firefox,
                extension_id: id.clone(),
            });
        }
        Err(Error::UnknownCaller)
    }

    /// Whether the desktop answers this extension.
    pub fn is_allowed(&self) -> bool {
        match self.family {
            Family::Chromium => CHROMIUM_IDS.contains(&self.extension_id.as_str()),
            Family::Firefox => FIREFOX_IDS.contains(&self.extension_id.as_str()),
        }
    }
}

/// A Chromium extension id: 32 letters from `a` to `p`.
fn is_chromium_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// The first frame the host sends the desktop.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostHello {
    pub v: u32,
    /// The host's own version.
    pub host: String,
    pub caller: Caller,
}

impl HostHello {
    /// The hello for `caller` from this build of the host.
    pub fn new(caller: Caller) -> Self {
        Self {
            v: HELLO_V,
            host: env!("CARGO_PKG_VERSION").to_string(),
            caller,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEV_CHROMIUM_ID, FIREFOX_ID};

    #[test]
    fn chromium_and_firefox_arguments_name_the_caller() {
        let c = Caller::from_args([format!("chrome-extension://{DEV_CHROMIUM_ID}/")]).unwrap();
        assert_eq!(c.family, Family::Chromium);
        assert_eq!(c.extension_id, DEV_CHROMIUM_ID);
        assert!(c.is_allowed());
        // Windows adds the parent window after the origin.
        let w = Caller::from_args([
            format!("chrome-extension://{DEV_CHROMIUM_ID}/"),
            "--parent-window=0".to_string(),
        ])
        .unwrap();
        assert_eq!(w, c);
        let f = Caller::from_args([
            "/home/me/.mozilla/native-messaging-hosts/design.snds.lockdown.json",
            FIREFOX_ID,
        ])
        .unwrap();
        assert_eq!(f.family, Family::Firefox);
        assert!(f.is_allowed());
    }

    #[test]
    fn anything_else_is_refused_or_not_allowed() {
        assert!(Caller::from_args(Vec::<String>::new()).is_err());
        assert!(Caller::from_args(["chrome-extension://not-an-id/"]).is_err());
        assert!(Caller::from_args(["--flag", "x"]).is_err());
        let other = Caller::from_args([format!("chrome-extension://{}/", "a".repeat(32))]).unwrap();
        assert!(!other.is_allowed());
        let addon = Caller::from_args(["/x/m.json", "someone-else@example.com"]).unwrap();
        assert!(!addon.is_allowed());
    }

    #[test]
    fn the_hello_is_plain_json() {
        let hello = HostHello::new(Caller {
            family: Family::Firefox,
            extension_id: FIREFOX_ID.into(),
        });
        let json = serde_json::to_value(&hello).unwrap();
        assert_eq!(json["v"], 1);
        assert_eq!(json["caller"]["family"], "firefox");
        assert_eq!(json["caller"]["extension_id"], FIREFOX_ID);
    }
}
