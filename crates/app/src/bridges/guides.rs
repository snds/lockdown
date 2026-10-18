//! Set-up cards in the security center (docs/plan/23-bridges.md §3): for a
//! login whose site offers two-step sign-in (or passkeys) and holds
//! neither, what the site offers, its own notes, and Lockdown's steps,
//! with the attribution both directories ask for. Built from bundled data
//! only: nobody learns which sites a person has logins for.

use lockdown_matcher::Origin;
use lockdown_security::guide::{Attribution, SetupGuide, setup_guide};
use lockdown_security::site_data::SiteData;
use lockdown_security::twofa::TwoFaDirectory;
use serde::Serialize;

use crate::{App, AppError, AppResult};

/// Where the guides' data comes from, for the Security screen's footer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GuideSources {
    pub sources: Vec<Attribution>,
    /// The signed data's version and date.
    pub version: u64,
    pub published: String,
}

impl App {
    /// The set-up card for an item's site, when either directory knows it.
    pub fn setup_guide(&self, item: &str) -> AppResult<Option<SetupGuide>> {
        let item = item.to_string();
        let site = self.call(move |core| {
            core.session()?
                .security_site(&item)
                .map_err(AppError::refused)
        })?;
        let Some(site) = site else {
            return Ok(None);
        };
        let host = Origin::parse(&site)
            .map(|o| o.host)
            .map_err(|_| AppError::refused("this item's website isn't a web address"))?;
        Ok(setup_guide(
            &host,
            TwoFaDirectory::builtin(),
            SiteData::builtin(),
        ))
    }

    /// The directories behind the cards, for attribution.
    pub fn guide_sources(&self) -> GuideSources {
        let data = SiteData::builtin();
        let twofa = TwoFaDirectory::builtin();
        let mut sources = vec![Attribution {
            name: "2fa.directory".into(),
            licence: twofa.licence.clone(),
            url: "https://github.com/2factorauth/twofactorauth".into(),
        }];
        sources.extend(
            data.payload()
                .sources
                .iter()
                .filter(|s| s.id != "2fa")
                .map(|s| Attribution {
                    name: s.name.clone(),
                    licence: s.licence.clone(),
                    url: s.url.clone(),
                }),
        );
        GuideSources {
            sources,
            version: data.version,
            published: data.published.clone(),
        }
    }
}
