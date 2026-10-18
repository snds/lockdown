//! Email aliases through the services people already use, and alias
//! monitoring (docs/plan/23-bridges.md §5, §9).
//!
//! **Keys.** Each service's API key is kept as an API-key item in the
//! **crown-jewel** class, with Lockdown's own field `lockdown.alias`
//! holding which service it is for, its address when self-hosted, its
//! domain, and the aliases made with it. Making an alias goes through the
//! `Use` gate (a fresh platform check, or the account password), counts as
//! a use of the item (D151), and sends the key to that service only, over
//! TLS, from the core; the webview and the extension get the new address,
//! never the key.
//!
//! **Monitoring.** With "Watch my aliases for breaches" on (off by
//! default; it tells the server which aliases you have), the addresses
//! made here and every saved user name on an alias service's domain are
//! sent to the account's server, whose breach job checks them beside the
//! account's own address (D134's feed, and its trade-off).

use lockdown_security::alias::{
    self, AliasError, AliasRequest, AliasService, plain_address, service_of_address,
};
use lockdown_security::http::HttpFetch;
use lockdown_vault::{FieldChange, Id};
use serde::{Deserialize, Serialize};

use crate::session::bridges::ALIAS_FIELD;
use crate::{App, AppError, AppResult, ApprovalKind, GateInput, session};

/// The most aliases one service's item remembers.
pub const MAX_MADE: usize = 500;

/// How long one call to a service may take.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// An alias made, as the service's item remembers it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Made {
    pub address: String,
    #[serde(default)]
    pub site: String,
    pub at: i64,
}

/// What `lockdown.alias` holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AliasConfig {
    pub service: AliasService,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub made: Vec<Made>,
}

/// A service Lockdown knows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ServiceInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// Where to find the key, in words.
    pub key_hint: &'static str,
    pub needs_domain: bool,
    pub takes_domain: bool,
    pub self_hostable: bool,
}

/// A service the person set up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Configured {
    pub item_id: String,
    pub title: String,
    pub service: &'static str,
    pub service_name: &'static str,
    pub domain: Option<String>,
    /// Its own address, when not the service's.
    pub base: Option<String>,
    /// How many aliases were made with it.
    pub made: usize,
}

/// The catalogue and what is set up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AliasOverview {
    pub services: Vec<ServiceInfo>,
    pub configured: Vec<Configured>,
}

/// A new alias.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AliasMade {
    pub address: String,
    pub service: &'static str,
    pub service_name: &'static str,
    /// When monitoring is on and the server couldn't be told, why.
    pub monitoring_problem: Option<String>,
}

/// What monitoring covers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MonitoringView {
    pub enabled: bool,
    /// The addresses it covers (or would), sorted.
    pub addresses: Vec<String>,
    /// Whether there is an account (and so a server) to watch them.
    pub has_account: bool,
}

fn service_info(s: AliasService) -> ServiceInfo {
    ServiceInfo {
        id: s.id(),
        name: s.name(),
        key_hint: s.key_hint(),
        needs_domain: s.needs_domain(),
        takes_domain: s.takes_domain(),
        self_hostable: s.self_hostable(),
    }
}

fn config_of(text: &str) -> Option<AliasConfig> {
    serde_json::from_str(text).ok()
}

fn alias_error(e: AliasError) -> AppError {
    AppError::refused(e.to_string())
}

impl crate::Core {
    /// The monitored set: aliases made here and saved user names on an
    /// alias service's domain.
    fn monitored_addresses(&mut self) -> AppResult<Vec<String>> {
        let session = self.session()?;
        let mut all: Vec<String> = session
            .bridge_alias_items()
            .map_err(AppError::refused)?
            .iter()
            .filter_map(|i| config_of(&i.config))
            .flat_map(|c| c.made.into_iter().map(|m| m.address))
            .collect();
        all.extend(
            session
                .bridge_logins()
                .map_err(AppError::refused)?
                .into_iter()
                .map(|f| f.username.trim().to_ascii_lowercase())
                .filter(|u| service_of_address(u).is_some()),
        );
        all.retain(|a| plain_address(a));
        for a in &mut all {
            *a = a.to_ascii_lowercase();
        }
        all.sort();
        all.dedup();
        all.truncate(lockdown_sync::wire::MAX_MONITORED);
        Ok(all)
    }
}

impl App {
    /// The services Lockdown makes aliases with, and the ones set up.
    pub fn alias_overview(&self) -> AppResult<AliasOverview> {
        self.call(|core| {
            let items = core
                .session()?
                .bridge_alias_items()
                .map_err(AppError::refused)?;
            let mut configured: Vec<Configured> = items
                .iter()
                .filter_map(|i| {
                    let c = config_of(&i.config)?;
                    Some(Configured {
                        item_id: i.item_id.to_string(),
                        title: i.title.clone(),
                        service: c.service.id(),
                        service_name: c.service.name(),
                        domain: c.domain.clone(),
                        base: c.base.clone(),
                        made: c.made.len(),
                    })
                })
                .collect();
            configured.sort_by(|a, b| {
                a.service_name
                    .cmp(b.service_name)
                    .then(a.title.cmp(&b.title))
            });
            Ok(AliasOverview {
                services: AliasService::ALL.into_iter().map(service_info).collect(),
                configured,
            })
        })
    }

    /// Sets up a service: its key as a new crown-jewel API-key item.
    /// `base` is a self-hosted (or test) address; `domain` the person's
    /// own, where the service takes one. Returns the item's id.
    pub fn alias_service_add(
        &self,
        service: &str,
        key: Vec<u8>,
        base: Option<&str>,
        domain: Option<&str>,
    ) -> AppResult<String> {
        let key = zeroize::Zeroizing::new(key);
        let service = AliasService::from_id(service)
            .ok_or_else(|| AppError::refused("that isn't an alias service Lockdown knows"))?;
        let trimmed = std::str::from_utf8(&key)
            .map(str::trim)
            .map_err(|_| AppError::refused("the API key should be text"))?;
        if trimmed.is_empty()
            || trimmed.len() > 512
            || !trimmed.chars().all(|c| c.is_ascii_graphic())
        {
            return Err(AppError::refused(
                "the API key should be one line of letters, digits and symbols",
            ));
        }
        let base = base
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(|b| b.trim_end_matches('/').to_string());
        if let Some(b) = &base {
            lockdown_security::http::allowed_address(b)
                .map_err(|e| AppError::refused(e.to_string()))?;
        }
        let domain = domain
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(str::to_ascii_lowercase);
        if service.needs_domain() && domain.is_none() {
            return Err(AppError::refused(format!(
                "{} makes aliases on one of your domains; name it",
                service.name()
            )));
        }
        let host = base
            .as_deref()
            .unwrap_or(service.default_base())
            .split("://")
            .nth(1)
            .unwrap_or("")
            .split(['/', ':'])
            .next()
            .unwrap_or("")
            .to_string();
        let id = self.add_item(
            "api-credential",
            Some(&format!("{} API key", service.name())),
            true,
            vec![
                FieldChange::secret("api.key", trimmed.as_bytes()),
                FieldChange::text("api.host", &host),
                FieldChange::text("api.scopes", "Email aliases"),
            ],
        )?;
        let config = AliasConfig {
            service,
            base,
            domain,
            made: Vec::new(),
        };
        let text = serde_json::to_string(&config).map_err(|e| AppError::refused(e.to_string()))?;
        let item = Id::parse(&id).map_err(|_| AppError::refused("the new item has no id"))?;
        self.call(move |core| {
            core.session()?
                .bridge_set(item, ALIAS_FIELD, Some(text), &[])
                .map_err(AppError::refused)
        })?;
        Ok(id)
    }

    /// Makes a new alias with the service set up as `item`, for `site` (a
    /// host name the service notes beside it), through the `Use` gate.
    pub fn alias_generate(&self, item: &str, site: &str, gate: GateInput) -> AppResult<AliasMade> {
        let item_text = item.to_string();
        let item_id = Id::parse(item).map_err(|_| AppError::refused("that isn't an item"))?;
        let site: String = site
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split(['/', '?', '#', ':'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let (key, config) = self.call(move |core| {
            // The key is a crown jewel; even one moved out of the class by
            // hand is asked for the same way.
            core.clock.touch(std::time::Instant::now());
            core.approve(ApprovalKind::Use, "use your alias service's key", &gate)?;
            let (key, config) = core
                .session()?
                .bridge_alias_key(&item_text)
                .map_err(AppError::refused)?;
            core.note_use(&item_text);
            Ok((key, config))
        })?;
        let mut config = config_of(&config)
            .ok_or_else(|| AppError::refused("this alias service's settings didn't read"))?;
        let fetch = HttpFetch::new(TIMEOUT).map_err(|e| AppError::refused(e.to_string()))?;
        let made = alias::create_alias(
            &fetch,
            &AliasRequest {
                service: config.service,
                base: config.base.as_deref(),
                domain: config.domain.as_deref(),
                site: &site,
                key: &key,
            },
        )
        .map_err(alias_error)?;
        drop(key);
        config.made.push(Made {
            address: made.address.clone(),
            site,
            at: session::now_secs(),
        });
        if config.made.len() > MAX_MADE {
            let extra = config.made.len() - MAX_MADE;
            config.made.drain(..extra);
        }
        let text = serde_json::to_string(&config).map_err(|e| AppError::refused(e.to_string()))?;
        let monitoring = self.call(move |core| {
            core.session()?
                .bridge_set(item_id, ALIAS_FIELD, Some(text), &[])
                .map_err(AppError::refused)?;
            Ok(core.settings.alias_monitoring)
        })?;
        let monitoring_problem = if monitoring {
            self.push_monitored().err().map(|e| e.message)
        } else {
            None
        };
        Ok(AliasMade {
            address: made.address,
            service: config.service.id(),
            service_name: config.service.name(),
            monitoring_problem,
        })
    }

    /// What alias monitoring covers.
    pub fn alias_monitoring(&self) -> AppResult<MonitoringView> {
        self.call(|core| {
            let addresses = core.monitored_addresses()?;
            Ok(MonitoringView {
                enabled: core.settings.alias_monitoring,
                has_account: core.session()?.account_session().is_some(),
                addresses,
            })
        })
    }

    /// Turns alias monitoring on (sending the list to the server) or off
    /// (sending an empty one).
    pub fn alias_monitoring_set(&self, on: bool) -> AppResult<MonitoringView> {
        let has_account = self.call(|core| Ok(core.session()?.account_session().is_some()))?;
        if on && !has_account {
            return Err(AppError::refused(
                "monitoring runs on your account's server; add an account on Status first",
            ));
        }
        self.call(move |core| {
            let mut settings = core.settings.clone();
            settings.alias_monitoring = on;
            settings
                .save(&core.paths.settings_file())
                .map_err(AppError::refused)?;
            core.settings = settings;
            Ok(())
        })?;
        if has_account {
            self.push_monitored()?;
        }
        self.alias_monitoring()
    }

    /// Sends the monitored set (empty when monitoring is off) to the server.
    fn push_monitored(&self) -> AppResult<usize> {
        let (account, list) = self.call(|core| {
            let account = core
                .session()?
                .account_session()
                .ok_or_else(|| AppError::refused("no account on this device"))?
                .map_err(AppError::refused)?;
            let list = if core.settings.alias_monitoring {
                core.monitored_addresses()?
            } else {
                Vec::new()
            };
            Ok((account, list))
        })?;
        push(&account.0, &account.1, list)
    }
}

fn push(server: &str, token: &str, addresses: Vec<String>) -> AppResult<usize> {
    let transport =
        lockdown_sync::HttpTransport::new(server).map_err(|e| AppError::refused(e.to_string()))?;
    let back = transport
        .set_monitored(
            token,
            &lockdown_sync::wire::MonitoredAddresses { addresses },
        )
        .map_err(|e| {
            AppError::refused(format!(
                "the server couldn't be told which aliases to watch: {e}"
            ))
        })?;
    Ok(back.addresses.len())
}
