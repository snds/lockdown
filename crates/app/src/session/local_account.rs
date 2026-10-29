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

    /// Whether this device's keys are sealed under a master password.
    pub(crate) fn keys_sealed(&self) -> bool {
        local_unlock::key_file(&self.paths.data) == local_unlock::KeyFile::Sealed
    }
}
