//! `lockdown-bridge-host`: the bridge host (03-architecture §2 `bridge-host`
//! row; brief §2.9 points 1–3, 6, 7; D6, D8).
//!
//! **Status: skeleton with the tier gate real.** The manifest shape, the
//! four tiers and the load gate (closed is never loaded; tolerated needs a
//! recorded consent; a killed bridge is refused) are implemented and tested.
//! Manifest signature checking, the remote kill-switch feed and the only v1
//! bridge (device-code activation) are not built yet. In v1 the host is
//! otherwise empty (D8).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A bridge's risk tier (brief §2.9 point 2).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BridgeTier {
    /// Passkeys, TOTP, the sign-in-with map. Always on.
    Standards,
    /// An official vendor flow such as device-code activation. On, per use,
    /// behind the proximity rule.
    Official,
    /// Reverse-engineered but tolerated (Steam QR approval). Off until the
    /// person reads a disclosure and opts in.
    Tolerated,
    /// Vendor push emulation. Never built, never loaded.
    Closed,
}

/// How dangerous a secret the bridge stores is (02-model §3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretClass {
    /// An ordinary secret.
    Standard,
    /// High value; a second deliberate action before use.
    High,
    /// The crown-jewel class: its own key, never autofilled or exported by default.
    CrownJewel,
}

/// One secret the bridge keeps.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SecretSpec {
    /// Its class.
    pub class: SecretClass,
    /// Shown but never used by the bridge (for example a revocation code).
    #[serde(default)]
    pub display_only: bool,
}

/// The signed capability manifest every bridge ships (brief §2.9 point 1).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct BridgeManifest {
    /// The bridge's id, such as `device-code`.
    pub id: String,
    /// Its tier.
    pub tier: BridgeTier,
    /// What it can do; the UI draws itself from this list.
    pub capabilities: Vec<String>,
    /// The secrets it stores, by name.
    #[serde(default)]
    pub secrets: BTreeMap<String, SecretSpec>,
    /// The only hosts it may talk to.
    #[serde(default)]
    pub network_allowlist: Vec<String>,
    /// The disclosures the person must read before a tolerated bridge turns on.
    #[serde(default)]
    pub disclosures: Vec<String>,
    /// Capabilities that go through the approval service's proximity gate.
    #[serde(default)]
    pub requires_proximity_gate: Vec<String>,
    /// Whether the bridge can be switched off remotely by a signed configuration.
    pub kill_switch: bool,
    /// The version of the disclosure text this manifest ships.
    #[serde(default)]
    pub disclosure_version: u32,
}

/// A recorded consent for a tolerated bridge: the person read disclosure
/// `disclosure_version` and said yes (brief §2.9 point 3). The identity
/// crate's `Consent` record is the source of truth; this is its shape here.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ConsentRecord {
    /// The bridge id.
    pub bridge: String,
    /// The disclosure version that was shown.
    pub disclosure_version: u32,
}

/// Why a bridge did not load.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LoadError {
    /// Closed-tier bridges are never built or loaded.
    #[error("bridge '{0}' is in the closed tier; Lockdown never loads closed bridges")]
    ClosedTier(String),
    /// A tolerated bridge needs the person's recorded consent first.
    #[error("bridge '{0}' needs your consent to disclosure version {1} before it turns on")]
    ConsentRequired(String, u32),
    /// A kill switch was thrown for this bridge.
    #[error("bridge '{0}' has been switched off: {1}")]
    Killed(String, String),
    /// A manifest that must have a kill switch lacks one.
    #[error("bridge '{0}' must carry a kill switch at its tier")]
    NoKillSwitch(String),
    /// The feature exists in the plan but not in this build yet.
    #[error("not supported here: {0}")]
    Unsupported(String),
}

/// The host: which bridges are loaded, and which are killed.
#[derive(Default, Debug)]
pub struct BridgeHost {
    loaded: BTreeMap<String, BridgeManifest>,
    killed: BTreeMap<String, String>,
}

impl BridgeHost {
    /// An empty host.
    pub fn new() -> Self {
        Self::default()
    }

    /// Throws the kill switch for `bridge` with a reason the person reads.
    pub fn kill(&mut self, bridge: &str, reason: &str) {
        self.killed.insert(bridge.to_string(), reason.to_string());
        self.loaded.remove(bridge);
    }

    /// Loads `manifest` if its tier and gates allow. `consent` is the
    /// person's recorded consent for this bridge, if any.
    pub fn load(
        &mut self,
        manifest: BridgeManifest,
        consent: Option<&ConsentRecord>,
    ) -> Result<(), LoadError> {
        if let Some(reason) = self.killed.get(&manifest.id) {
            return Err(LoadError::Killed(manifest.id.clone(), reason.clone()));
        }
        match manifest.tier {
            BridgeTier::Closed => return Err(LoadError::ClosedTier(manifest.id)),
            BridgeTier::Tolerated | BridgeTier::Official if !manifest.kill_switch => {
                return Err(LoadError::NoKillSwitch(manifest.id));
            }
            BridgeTier::Tolerated => {
                let ok = consent.is_some_and(|c| {
                    c.bridge == manifest.id && c.disclosure_version >= manifest.disclosure_version
                });
                if !ok {
                    return Err(LoadError::ConsentRequired(
                        manifest.id,
                        manifest.disclosure_version,
                    ));
                }
            }
            BridgeTier::Standards | BridgeTier::Official => {}
        }
        self.loaded.insert(manifest.id.clone(), manifest);
        Ok(())
    }

    /// The loaded manifest for `bridge`, if any.
    pub fn get(&self, bridge: &str) -> Option<&BridgeManifest> {
        self.loaded.get(bridge)
    }

    /// Whether `capability` of `bridge` must go through the proximity gate.
    pub fn needs_proximity_gate(&self, bridge: &str, capability: &str) -> bool {
        self.get(bridge)
            .is_some_and(|m| m.requires_proximity_gate.iter().any(|c| c == capability))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, tier: BridgeTier) -> BridgeManifest {
        BridgeManifest {
            id: id.into(),
            tier,
            capabilities: vec!["approve_qr_login".into()],
            secrets: BTreeMap::new(),
            network_allowlist: vec![],
            disclosures: vec![],
            requires_proximity_gate: vec!["approve_qr_login".into()],
            kill_switch: true,
            disclosure_version: 2,
        }
    }

    #[test]
    fn the_tier_gate_refuses_closed_and_unconsented() {
        let mut host = BridgeHost::new();
        assert_eq!(
            host.load(manifest("push-emulation", BridgeTier::Closed), None),
            Err(LoadError::ClosedTier("push-emulation".into()))
        );
        assert_eq!(
            host.load(manifest("steam", BridgeTier::Tolerated), None),
            Err(LoadError::ConsentRequired("steam".into(), 2))
        );
        let stale = ConsentRecord {
            bridge: "steam".into(),
            disclosure_version: 1,
        };
        assert!(matches!(
            host.load(manifest("steam", BridgeTier::Tolerated), Some(&stale)),
            Err(LoadError::ConsentRequired(..))
        ));
        let current = ConsentRecord {
            bridge: "steam".into(),
            disclosure_version: 2,
        };
        host.load(manifest("steam", BridgeTier::Tolerated), Some(&current))
            .unwrap();
        assert!(host.needs_proximity_gate("steam", "approve_qr_login"));
        host.load(manifest("device-code", BridgeTier::Official), None)
            .unwrap();
        let mut no_switch = manifest("device-code-2", BridgeTier::Official);
        no_switch.kill_switch = false;
        assert!(matches!(
            host.load(no_switch, None),
            Err(LoadError::NoKillSwitch(_))
        ));
    }

    #[test]
    fn the_kill_switch_unloads_and_blocks_reload() {
        let mut host = BridgeHost::new();
        host.load(manifest("device-code", BridgeTier::Official), None)
            .unwrap();
        host.kill("device-code", "vendor changed the activation page");
        assert!(host.get("device-code").is_none());
        assert!(matches!(
            host.load(manifest("device-code", BridgeTier::Official), None),
            Err(LoadError::Killed(..))
        ));
    }
}
