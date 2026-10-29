//! The local account's pieces on an open session (docs/plan/39-first-run.md
//! §1.2, §1.6, §1.10): which account this is, the master password checked
//! against this computer's own slot, and the account password changed on
//! the account's server after a forgotten one.

use lockdown_crypto::auth::{ACCOUNT_KSF, AccountKsf, RegistrationClient};
use lockdown_sync::HttpTransport;
use lockdown_sync::account_security::PasswordFinish;
use serde_bytes::ByteBuf;

use super::{PreparedExportKey, Session, account_envelope, sync_identity_records};
use crate::local_unlock::{self, DeviceKeys};
use ecosystem_identity::records::{KeyRef, KeyRefKind};

impl Session {
    /// This device's keys.
    pub(crate) fn device_keys(&self) -> &DeviceKeys {
        &self.keystore
    }

    /// Whether the account is the local one the app keeps itself.
    pub(crate) fn is_local_account(&self) -> bool {
        self.sync
            .as_ref()
            .is_some_and(|c| lockdown_sync::is_local(&c.server))
    }

    /// The account's login, when there is an account.
    pub(crate) fn account_login_name(&self) -> Option<String> {
        self.sync.as_ref().map(|c| c.login.clone())
    }

    /// Whether `password` opens this computer's password slot (a sealed
    /// install), without a server.
    pub(crate) fn check_local_password(&self, password: &str) -> Result<bool, String> {
        DeviceKeys::check_password(&self.paths.data, password)
    }

    /// Makes the account's password file again on its server under
    /// `password`, with the stretching it has now, and writes the account
    /// slot again under the new export key with the Secret Key this device
    /// keeps (a forgotten password, §1.10 K2). The session's token comes
    /// from the device's key (D400's renewal), so the old password is not
    /// needed.
    pub(crate) fn change_account_password(&mut self, password: &str) -> Result<(), String> {
        let sk = match self.secret_key_state() {
            lockdown_vault::device_link::SecretKeyState::Held(sk) => Some(sk),
            lockdown_vault::device_link::SecretKeyState::OptedOut => None,
            lockdown_vault::device_link::SecretKeyState::NotHeld => {
                return Err(
                    "This computer doesn't keep the Secret Key, which your account needs. Run the recovery drill first."
                        .into(),
                );
            }
        };
        let current = self.account_ksf_floor().unwrap_or(ACCOUNT_KSF);
        let current = current.max(&ACCOUNT_KSF);
        let cfg = self.sync.as_ref().ok_or("no account on this device")?;
        let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
        let token = self.token()?;
        let (client, request) = RegistrationClient::start(password).map_err(|e| e.to_string())?;
        let reply = transport
            .password_start(&token, &request)
            .map_err(|e| e.to_string())?;
        let ksf = AccountKsf::at(current).map_err(|e| e.to_string())?;
        let (upload, export_key) = client
            .finish_stretched(password, &reply.response, &ksf)
            .map_err(|e| e.to_string())?;
        transport
            .password_finish(
                &token,
                &PasswordFinish {
                    upload: ByteBuf::from(upload),
                    ksf: current,
                },
            )
            .map_err(|e| e.to_string())?;
        let profile_id = self.identity.profile().profile_id;
        if account_envelope(&self.identity).is_some() {
            let irk_id = self.identity.irk_id().map_err(|e| e.to_string())?;
            self.identity
                .retire_envelope(irk_id, KeyRef::new(KeyRefKind::Account, profile_id))
                .map_err(|e| e.to_string())?;
        }
        let mut prepared = PreparedExportKey(Some(export_key));
        self.identity
            .enable_account_unlock(&mut prepared, password, sk.as_ref())
            .map_err(|e| e.to_string())?;
        self.memory
            .remember_identity(self.identity.head())
            .map_err(|e| e.to_string())?;
        sync_identity_records(
            &transport,
            &token,
            &mut self.identity,
            self.keystore.as_ref(),
            &mut self.synced,
            &mut self.memory,
        )
    }

    /// Seals this device's keys under `password`, keeping the kit slot or
    /// writing it for `kit`.
    pub(crate) fn seal_keys(
        &self,
        password: &str,
        kit: Option<&ecosystem_identity::RecoveryKit>,
    ) -> Result<(), String> {
        self.keystore.seal(&self.paths.data, password, kit)
    }

    /// The identity's profile.
    pub(crate) fn profile_id(&self) -> lockdown_vault::Id {
        self.identity.profile().profile_id
    }

    /// Whether the identity has a recovery kit.
    pub(crate) fn has_kit(&self) -> bool {
        self.identity.profile().recovery != ecosystem_identity::records::RecoveryState::None
    }

    /// Moves the local account to the server at `server` (docs/plan/39-first-run.md
    /// §1.11, D473): the same identity, kit and vaults, registered there
    /// under `login` with the master password and the Secret Key kept
    /// here, then every vault pushed from its first head. Only once the
    /// push finished does this device use that server; on any failure it
    /// stays on the vault kept here, unchanged apart from the account slot
    /// written again for the server inside the app.
    pub(crate) fn move_to_server(
        &mut self,
        server: &str,
        login: &str,
        password: &str,
    ) -> Result<crate::types::SyncOutcome, String> {
        if !self.is_local_account() {
            return Err("This vault already syncs with a server.".into());
        }
        if !self.check_local_password(password)? {
            return Err("That isn't your master password.".into());
        }
        let sk = match self.secret_key_state() {
            lockdown_vault::device_link::SecretKeyState::Held(sk) => sk,
            _ => {
                return Err(
                    "This computer doesn't keep the Secret Key, which the move needs. Run the recovery drill first."
                        .into(),
                );
            }
        };
        let local = self.sync.clone();
        // The identity's signed record set as the server inside the app
        // accepted it is that server's; the new one starts its own.
        let synced_file = self.paths.data.join(super::IDENTITY_SYNC_FILE);
        let parked = synced_file.with_extension("cbor.before-move");
        let isk_pub = self.identity.profile().isk_pub;
        if synced_file.exists() {
            std::fs::rename(&synced_file, &parked).map_err(|e| e.to_string())?;
        }
        self.synced = lockdown_store::SyncedIdentity::open(&synced_file, Some(&isk_pub))
            .map_err(|e| e.to_string())?;
        // The account slot is bound to one server's OPRF key, so the slot is
        // written again for the new server (R5): retire the local one first.
        let profile_id = self.identity.profile().profile_id;
        if account_envelope(&self.identity).is_some() {
            let irk_id = self.identity.irk_id().map_err(|e| e.to_string())?;
            self.identity
                .retire_envelope(irk_id, KeyRef::new(KeyRefKind::Account, profile_id))
                .map_err(|e| e.to_string())?;
        }
        let restore = |session: &mut Session, why: String| -> String {
            // Back to the local account: its slot written again with the
            // same password, so the gates and a new kit keep working here.
            session.sync = local.clone();
            if parked.exists() {
                let _ = std::fs::rename(&parked, &synced_file);
            }
            if let Ok(synced) = lockdown_store::SyncedIdentity::open(&synced_file, Some(&isk_pub)) {
                session.synced = synced;
            }
            if let Some(cfg) = &local {
                let _ = cfg.save(&session.paths.sync_file());
            }
            if account_envelope(&session.identity).is_none()
                && let Some(cfg) = &local
                && let Ok(transport) = HttpTransport::new(&cfg.server)
            {
                let mut auth = lockdown_crypto::auth::OpaqueAccountAuth::new(
                    transport,
                    &cfg.login,
                    session.device_binding(),
                );
                if let Ok((export_key, _)) = auth.sign_in(password) {
                    let mut prepared = PreparedExportKey(Some(export_key));
                    let _ =
                        session
                            .identity
                            .enable_account_unlock(&mut prepared, password, Some(&sk));
                    let _ = session.memory.remember_identity(session.identity.head());
                }
            }
            format!(
                "The move stopped: {why}. Your vault is unchanged and still on this computer. You can try again."
            )
        };
        if let Err(e) = self.account_register_with(server, login, password, Some(sk)) {
            let words = if e.contains("taken") || e.contains("exists") {
                format!("{login} is already taken on that server. Choose another login.")
            } else if e.contains("signups") || e.contains("invite") || e.contains("closed") {
                "That server isn't taking new accounts. Ask whoever runs it for an invite."
                    .to_string()
            } else {
                e
            };
            return Err(restore(self, words));
        }
        match self.sync() {
            Ok(outcome) => {
                // The server inside the app is no longer used; its folder goes
                // (best effort: a process may still hold it open).
                let _ = std::fs::remove_dir_all(self.paths.data.join("local-server"));
                let _ = std::fs::remove_file(&parked);
                Ok(outcome)
            }
            Err(e) => Err(restore(self, e)),
        }
    }

    /// Whether this device's keys are sealed under a master password.
    pub(crate) fn keys_sealed(&self) -> bool {
        local_unlock::key_file(&self.paths.data) == local_unlock::KeyFile::Sealed
    }
}
