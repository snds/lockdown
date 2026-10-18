//! Set-up cards for two-step sign-in and passkeys (docs/plan/23-bridges.md
//! §3): what a site offers (the bundled 2fa.directory snapshot), the
//! site's own notes and recovery page (2fa.directory) and its passkey
//! support (the Passkeys Directory), with Lockdown's steps in plain words
//! and the attribution each source asks for.
//!
//! The directories' own sentences are shown as theirs (quoted, with the
//! source named), never rewritten; Lockdown's steps are written here.

use serde::Serialize;

use crate::site_data::SiteData;
use crate::twofa::TwoFaDirectory;

/// One source of a card, as its attribution line says it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Attribution {
    /// The source's name.
    pub name: String,
    /// Its licence.
    pub licence: String,
    /// Where it lives.
    pub url: String,
}

/// A site's set-up card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SetupGuide {
    /// The site's name.
    pub site: String,
    /// What it offers, strongest first, in plain words.
    pub methods: Vec<String>,
    /// Whether it takes an authenticator app's code, which Lockdown holds.
    pub app_codes: bool,
    /// Passkey support in words, when it has any.
    pub passkeys: Option<String>,
    /// The site's own page on turning two-step sign-in on.
    pub setup_url: Option<String>,
    /// Its page on passkeys.
    pub passkey_url: Option<String>,
    /// Its account-recovery page.
    pub recovery_url: Option<String>,
    /// The site's own notes, as the directory has them.
    pub notes: Option<String>,
    /// The names of the site's own apps or devices.
    pub site_apps: Vec<String>,
    /// What to do, in order; the last step checks it worked.
    pub steps: Vec<String>,
    /// Where the details came from.
    pub attribution: Vec<Attribution>,
}

fn attribution(data: &SiteData, id: &str) -> Option<Attribution> {
    data.source(id).map(|s| Attribution {
        name: s.name.clone(),
        licence: s.licence.clone(),
        url: s.url.clone(),
    })
}

/// The card for `host`, when either directory knows the site.
pub fn setup_guide(host: &str, twofa: &TwoFaDirectory, data: &SiteData) -> Option<SetupGuide> {
    let entry = twofa.lookup(host);
    let extra = data.guide(host);
    if entry.is_none() && extra.is_none() {
        return None;
    }
    let site = entry
        .map(|e| e.name.clone())
        .or_else(|| extra.map(|g| g.n.clone()))
        .unwrap_or_else(|| host.to_string());
    let methods: Vec<String> = entry
        .map(|e| e.method_words().iter().map(|w| w.to_string()).collect())
        .unwrap_or_default();
    let app_codes = entry.is_some_and(|e| e.takes_app_codes());
    let passkey = extra.and_then(|g| g.passkey.as_deref());
    let passkeys = passkey.map(|p| {
        match (p.contains("passwordless"), p.contains("mfa")) {
            (true, true) => {
                "It takes a passkey to sign in without a password, or as the second step."
            }
            (true, false) => "It takes a passkey to sign in without a password.",
            _ => "It takes a passkey as the second step after your password.",
        }
        .to_string()
    });
    let setup_url = entry.and_then(|e| e.documentation.clone());
    let mut site_apps: Vec<String> = extra
        .map(|g| g.apps.iter().chain(&g.devices).cloned().collect())
        .unwrap_or_default();
    site_apps.dedup();

    let mut steps = Vec::new();
    if extra.and_then(|g| g.notes.as_ref()).is_some() {
        steps.push(format!(
            "Read {site}'s own note below first: it says what the site needs before you start."
        ));
    }
    steps.push(match &setup_url {
        Some(_) => format!("Open {site}'s set-up page with Set it up and sign in there."),
        None => format!(
            "Sign in to {site} and look for two-step sign-in (it may be called two-factor authentication) in its security settings."
        ),
    });
    if app_codes {
        steps.push(
            "Choose the authenticator app. When the site shows a QR code, choose to see the set-up key instead, copy it, and add it to this login in Lockdown (Edit, One-time code)."
                .into(),
        );
        steps.push("Type the six-digit code Lockdown now shows into the site to finish.".into());
    } else if !methods.is_empty() {
        steps.push(format!(
            "Choose one of what {site} offers: {}.",
            methods.join(", ")
        ));
    }
    if passkeys.is_some() {
        steps.push(format!(
            "{site} takes passkeys too: with Lockdown's browser extension, make one on the site's security page and Lockdown keeps it with this login."
        ));
    }
    steps.push(
        "Save the backup codes the site gives you in a Note in Lockdown, so you can get in if you lose this device."
            .into(),
    );
    steps.push(
        "Sign out of the site and back in: it should ask for the second step. Then choose Check now; this card goes away once a code or passkey is saved with the login."
            .into(),
    );

    let mut attribution_lines = Vec::new();
    if entry.is_some() || extra.is_some_and(|g| g.src.iter().any(|s| s == "2fa")) {
        attribution_lines.push(Attribution {
            name: "2fa.directory".into(),
            licence: twofa.licence.clone(),
            url: "https://github.com/2factorauth/twofactorauth".into(),
        });
    }
    if extra.is_some_and(|g| g.src.iter().any(|s| s == "passkeys"))
        && let Some(a) = attribution(data, "passkeys")
    {
        attribution_lines.push(a);
    }

    Some(SetupGuide {
        site,
        methods,
        app_codes,
        passkeys,
        setup_url,
        passkey_url: extra.and_then(|g| g.passkey_doc.clone()),
        recovery_url: extra.and_then(|g| g.recovery.clone()),
        notes: extra.and_then(|g| g.notes.clone()),
        site_apps,
        steps,
        attribution: attribution_lines,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_joins_both_directories_and_names_them() {
        let twitch = setup_guide(
            "www.twitch.tv",
            TwoFaDirectory::builtin(),
            SiteData::builtin(),
        )
        .unwrap();
        assert_eq!(twitch.site, "Twitch");
        assert!(twitch.app_codes);
        assert!(twitch.notes.as_deref().unwrap().contains("phone number"));
        assert!(twitch.steps[0].contains("own note"));
        assert!(twitch.steps.last().unwrap().contains("Check now"));
        assert_eq!(twitch.attribution[0].name, "2fa.directory");

        let github =
            setup_guide("github.com", TwoFaDirectory::builtin(), SiteData::builtin()).unwrap();
        assert!(
            github
                .passkeys
                .as_deref()
                .unwrap()
                .contains("without a password")
        );
        assert!(
            github
                .passkey_url
                .as_deref()
                .unwrap()
                .starts_with("https://")
        );
        assert!(github.recovery_url.is_some());
        assert!(github.site_apps.iter().any(|a| a == "GitHub Mobile"));
        let names: Vec<&str> = github.attribution.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["2fa.directory", "Passkeys Directory"]);
        assert_eq!(github.attribution[1].licence, "CC BY 4.0");
        // A site neither directory knows has no card.
        assert!(
            setup_guide(
                "unknown.example",
                TwoFaDirectory::builtin(),
                SiteData::builtin()
            )
            .is_none()
        );
    }

    #[test]
    fn the_steps_are_plain_words() {
        for host in [
            "twitch.tv",
            "github.com",
            "gog.com",
            "roblox.com",
            "ubisoft.com",
        ] {
            let g = setup_guide(host, TwoFaDirectory::builtin(), SiteData::builtin()).unwrap();
            for step in &g.steps {
                for jargon in ["TOTP", "U2F", "FIDO", "OTP", "MFA", "2FA"] {
                    assert!(!step.contains(jargon), "{host}: {step}");
                }
                assert!(step.len() < 260, "{host}: {step}");
            }
        }
    }
}
