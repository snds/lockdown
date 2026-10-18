//! Approving a new device from this one, and joining from another
//! (`device-approval`, `secret-key`; `lockdown_vault::device_link` has the
//! protocol, docs/plan/22-security-depth.md §2–§3 the flow).
//!
//! On the **existing device** a session holds at most one open link: made
//! with [`Session::link_offer`], read with [`Session::link_poll`] (which
//! answers the new device's request with the reveal as soon as it comes),
//! and finished with [`Session::link_approve`] after the `Approve` gate, or
//! [`Session::link_decline`]. Approving writes the new device's record,
//! binds it on the server, publishes the identity's records, and only then
//! sends the grant and the Welcome, so the new device's first sign-in finds
//! itself listed.
//!
//! On the **new device** there is no session yet: [`Joining`] holds the
//! keystore and the relay's key between the shell's calls. When the
//! Welcome checks out the identity is restored at once from the root key
//! it carried ([`Joining::poll`]); [`finish_joined`] then signs in with the
//! account password (the session, the records, the vaults), and keeps the
//! Secret Key once it has opened the account slot.

use std::collections::BTreeMap;
use std::rc::Rc;

use ecosystem_identity::FileKeystore;
use lockdown_crypto::auth::OpaqueAccountAuth;
use lockdown_store::{HeadMemory, SyncedIdentity};
use lockdown_sync::HttpTransport;
use lockdown_sync::account_security::{LinkDevice, LinkOpen, LinkSide};
use lockdown_vault::Id;
use lockdown_vault::device_link::{Join, JoinStep, LinkInvite, Offer, SecretKeyState, sas_words};
use lockdown_vault::identity::APP;
use serde::Serialize;
use serde_bytes::ByteBuf;

use super::{
    IDENTITY_SYNC_FILE, KEYSTORE_FILE, MEMORY_FILE, Paths, SYNC_V, Session, SyncConfig, binding_of,
    check_root_empty, device_name, secret_key, sync_identity_records,
};

/// What the existing device shows while a link is open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LinkView {
    /// `waiting` (for the new device), `compare` (the person compares the
    /// digits and approves), `approved`, `declined` or `closed`.
    pub state: String,
    /// The code to type on the new device: two groups of five.
    pub code: String,
    /// The QR payload (`lockdown:link:v1?…`): the server, the code and this
    /// device's fingerprint.
    pub qr: String,
    /// The same as a QR code's module rows, for the shell to draw
    /// (`crate::qr`); only while the link waits for the new device.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub qr_rows: Vec<String>,
    /// When the link closes on its own.
    pub expires_at: i64,
    /// The six digits, once the new device asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sas: Option<String>,
    /// What the new device calls itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// Its platform.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// Whether the Secret Key goes with the approval: `kept`, `opted-out`
    /// (the account has none) or `not-kept` (this device doesn't keep it;
    /// the new device asks for the kit when it needs it).
    pub secret_key: String,
    /// In plain words, what happens next.
    pub words: String,
}

/// The open link on the existing device.
pub(crate) struct OpenLink {
    offer: Option<Offer>,
    link_id: Id,
    code: String,
    qr: String,
    expires_at: i64,
    read: u32,
    asked: Option<(String, String, String)>,
    state: &'static str,
}

impl Session {
    fn link_view(&self, link: &OpenLink) -> LinkView {
        let sk = secret_key::state_word(&self.secret_key_state());
        let words = match link.state {
            "waiting" => {
                "On the new device, choose to join with another device and type this code, or scan the QR code with the web vault.".to_string()
            }
            "compare" => format!(
                "Check that {} shows {}. Approve only if the digits match.",
                link.asked.as_ref().map(|a| a.1.as_str()).unwrap_or("the new device"),
                link.asked.as_ref().map(|a| a.0.as_str()).unwrap_or("")
            ),
            "approved" => "Approved. The new device finishes by signing in with your account password.".into(),
            "declined" => "Declined. Nothing was changed.".into(),
            _ => "This link closed. Make a new code to try again.".into(),
        };
        LinkView {
            state: link.state.into(),
            code: link.code.clone(),
            qr: link.qr.clone(),
            qr_rows: if link.state == "waiting" {
                crate::qr::rows(&link.qr).0
            } else {
                Vec::new()
            },
            expires_at: link.expires_at,
            sas: link.asked.as_ref().map(|a| a.0.clone()),
            device_name: link.asked.as_ref().map(|a| a.1.clone()),
            platform: link.asked.as_ref().map(|a| a.2.clone()),
            secret_key: sk.into(),
            words,
        }
    }

    /// What this device can hand to a new one: its kept Secret Key, the
    /// account's opt-out, or nothing.
    pub(crate) fn secret_key_state(&self) -> SecretKeyState {
        secret_key::state(
            &self.paths.data,
            self.keystore.as_ref(),
            self.identity.profile().profile_id,
        )
    }

    /// Keeps a Secret Key this device just saw open the account slot.
    pub(crate) fn keep_secret_key(&self, sk: &[u8; 16]) -> Result<(), String> {
        secret_key::keep(
            &self.paths.data,
            self.keystore.as_ref(),
            self.identity.profile().profile_id,
            sk,
        )
    }

    /// Opens a link for a new device: a fresh code, its hash filed on the
    /// server with this device's Hello. A link already open is closed first.
    pub fn link_offer(&mut self) -> Result<LinkView, String> {
        let cfg = self
            .sync
            .as_ref()
            .ok_or("adding a device this way needs the account on this device; log in first")?;
        let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
        let server = cfg.server.clone();
        if let Some(old) = self.link.take() {
            let _ = transport.link_close(&token, old.link_id);
        }
        let (offer, hello) = Offer::new(&self.identity).map_err(|e| e.to_string())?;
        let opened = transport
            .link_open(
                &token,
                &LinkOpen {
                    token_hash: offer.code().token_hash(),
                    hello: ByteBuf::from(hello),
                },
            )
            .map_err(|e| e.to_string())?;
        let link = OpenLink {
            code: offer.code().display(),
            qr: offer.invite(Some(&server)).to_qr(),
            link_id: opened.link_id,
            expires_at: opened.expires_at,
            read: 0,
            asked: None,
            state: "waiting",
            offer: Some(offer),
        };
        let view = self.link_view(&link);
        self.link = Some(link);
        Ok(view)
    }

    /// Reads the relay: a new device's request is checked and answered with
    /// the reveal at once, and the digits come back for the person.
    pub fn link_poll(&mut self) -> Result<LinkView, String> {
        let mut link = self.link.take().ok_or("no device is being added")?;
        let result = self.link_poll_inner(&mut link);
        let view = self.link_view(&link);
        self.link = Some(link);
        result.map(|()| view)
    }

    fn link_poll_inner(&mut self, link: &mut OpenLink) -> Result<(), String> {
        if link.state != "waiting" {
            return Ok(());
        }
        let cfg = self.sync.as_ref().ok_or("no account on this device")?;
        let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
        let frames = transport
            .link_poll(link.link_id, LinkSide::Opener(&token), link.read)
            .map_err(|e| e.to_string())?;
        if let Some(frame) = frames.frames.first() {
            link.read += 1;
            let offer = link.offer.as_mut().ok_or("this link already finished")?;
            match offer.take_request(frame, &self.identity) {
                Ok(asked) => {
                    transport
                        .link_push(link.link_id, LinkSide::Opener(&token), &asked.reveal)
                        .map_err(|e| e.to_string())?;
                    link.asked = Some((sas_words(asked.sas), asked.device_name, asked.platform));
                    link.state = "compare";
                }
                Err((why, refusal)) => {
                    if let Some(r) = refusal {
                        let _ = transport.link_push(link.link_id, LinkSide::Opener(&token), &r);
                    }
                    let _ = transport.link_close(&token, link.link_id);
                    link.state = "closed";
                    return Err(why);
                }
            }
        } else if frames.closed || frames.expires_at <= lockdown_vault::Timestamp::now().0 {
            link.state = "closed";
        }
        Ok(())
    }

    /// The person compared the digits and approved (the caller ran the
    /// `Approve` gate): the new device is enrolled.
    pub fn link_approve(&mut self) -> Result<Vec<String>, String> {
        let mut link = self.link.take().ok_or("no device is being added")?;
        if link.state != "compare" {
            let state = link.state;
            self.link = Some(link);
            return Err(match state {
                "waiting" => "no device has asked yet".into(),
                _ => "this link already finished; make a new code".into(),
            });
        }
        let offer = link.offer.take().ok_or("this link already finished")?;
        let cfg = self.sync.as_ref().ok_or("no account on this device")?;
        let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
        let token = cfg.session_token()?.to_string();
        let sk = self.secret_key_state();
        let granted = offer
            .grant(&mut self.identity, self.keystore.as_ref(), &sk)
            .map_err(|e| e.to_string());
        let granted = match granted {
            Ok(g) => g,
            Err(e) => {
                link.state = "closed";
                self.link = Some(link);
                return Err(e);
            }
        };
        self.memory
            .remember_identity(granted.head)
            .map_err(|e| e.to_string())?;
        transport
            .link_device(
                &token,
                link.link_id,
                &LinkDevice {
                    device_id: granted.device.device_id,
                    device_pub: granted.device.device_pub,
                },
            )
            .map_err(|e| e.to_string())?;
        sync_identity_records(
            &transport,
            &token,
            &mut self.identity,
            self.keystore.as_ref(),
            &mut self.synced,
            &mut self.memory,
        )?;
        for frame in &granted.frames {
            transport
                .link_push(link.link_id, LinkSide::Opener(&token), frame)
                .map_err(|e| e.to_string())?;
        }
        self.refresh_trust();
        link.state = "approved";
        let name = granted.device.name.clone();
        let sk_line = match sk {
            SecretKeyState::Held(_) => "your Secret Key went with it, sealed to that device",
            SecretKeyState::OptedOut => "your account has no Secret Key, so none was sent",
            SecretKeyState::NotHeld => {
                "this device doesn't keep your Secret Key, so the new one asks for the kit when it needs it"
            }
        };
        self.link = Some(link);
        Ok(vec![
            format!("{name} is now one of your devices"),
            sk_line.to_string(),
        ])
    }

    /// The person said no, or closed the dialog: nothing is written.
    pub fn link_decline(&mut self) -> Result<(), String> {
        let Some(mut link) = self.link.take() else {
            return Ok(());
        };
        if let Some(cfg) = self.sync.as_ref()
            && let Ok(transport) = HttpTransport::new(&cfg.server)
            && let Ok(token) = cfg.session_token()
        {
            if link.state == "compare"
                && let Some(offer) = link.offer.as_ref()
                && let Ok(frame) = offer.decline()
            {
                let _ = transport.link_push(link.link_id, LinkSide::Opener(token), &frame);
            }
            let _ = transport.link_close(token, link.link_id);
        }
        link.state = "declined";
        Ok(())
    }
}

// --- the new device ------------------------------------------------------------------

/// What the new device shows while it joins.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct JoinView {
    /// `waiting` (for the other device's answer), `compare` (show the
    /// digits), `approved` (sign in with the password to finish), `done`.
    pub state: String,
    /// The six digits to compare.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sas: Option<String>,
    /// The server.
    pub server: String,
    /// Whether the Secret Key came: `sent`, `opted-out`, `not-held`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_key: Option<String>,
    /// In plain words.
    pub words: String,
}

/// A join in progress on a device with nothing on it.
pub struct Joining {
    paths: Paths,
    server: String,
    keystore: Rc<FileKeystore>,
    link_id: Id,
    key: Vec<u8>,
    join: Join,
    read: u32,
    state: &'static str,
    secret_key: Option<[u8; 16]>,
    secret_key_state: Option<&'static str>,
}

impl std::fmt::Debug for Joining {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Joining")
            .field("server", &self.server)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl Joining {
    /// Joins the link `invite` names (a scanned QR payload or a typed code)
    /// on `server` (the QR's, when it names one; it must then be the same).
    pub fn start(paths: &Paths, server: &str, invite: &str) -> Result<Self, String> {
        check_root_empty(paths)?;
        let invite = LinkInvite::parse(invite).map_err(|e| e.to_string())?;
        let server = match &invite.server {
            Some(s)
                if !server.trim().is_empty()
                    && s.trim_end_matches('/') != server.trim().trim_end_matches('/') =>
            {
                return Err(format!(
                    "the QR code is for {s}, not {server}; sign in to the server the other device uses"
                ));
            }
            Some(s) => s.trim_end_matches('/').to_string(),
            None if server.trim().is_empty() => {
                return Err("name the server the other device uses".into());
            }
            None => server.trim().trim_end_matches('/').to_string(),
        };
        let keystore = Rc::new(
            FileKeystore::open_or_create(&paths.data.join(KEYSTORE_FILE))
                .map_err(|e| e.to_string())?,
        );
        let transport = HttpTransport::new(&server).map_err(|e| e.to_string())?;
        let joined = transport
            .link_join(invite.code.as_str())
            .map_err(|e| e.to_string())?;
        let key = joined.key.into_vec();
        let frames = transport
            .link_poll(joined.link_id, LinkSide::Joiner(&key), 0)
            .map_err(|e| e.to_string())?;
        let hello = frames
            .frames
            .first()
            .ok_or("the other device hasn't said hello yet; try again")?;
        let (join, request) =
            Join::start(hello, keystore.as_ref(), &device_name(), invite.fingerprint)
                .map_err(|e| e.to_string())?;
        transport
            .link_push(joined.link_id, LinkSide::Joiner(&key), &request)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            paths: paths.clone(),
            server,
            keystore,
            link_id: joined.link_id,
            key,
            join,
            read: 1,
            state: "waiting",
            secret_key: None,
            secret_key_state: None,
        })
    }

    /// Where the join stands.
    pub fn view(&self) -> JoinView {
        let words = match self.state {
            "waiting" => "Waiting for the other device…".to_string(),
            "compare" => "Check that the other device shows the same six digits, then approve there.".into(),
            "approved" => match self.secret_key_state {
                Some("sent") => "Approved. Your Secret Key came with it. Sign in with your account password to finish.".into(),
                Some("opted-out") => "Approved. Your account has no Secret Key. Sign in with your account password to finish.".into(),
                _ => "Approved. Sign in with your account password to finish; the Secret Key from your kit is asked for only if it's ever needed.".into(),
            },
            _ => "Done.".into(),
        };
        JoinView {
            state: self.state.into(),
            sas: self.join.sas().map(sas_words),
            server: self.server.clone(),
            secret_key: self.secret_key_state.map(str::to_string),
            words,
        }
    }

    /// Reads the relay: the digits, then the grant and the Welcome, which
    /// restore the identity here.
    pub fn poll(&mut self) -> Result<JoinView, String> {
        if self.state == "approved" {
            return Ok(self.view());
        }
        let transport = HttpTransport::new(&self.server).map_err(|e| e.to_string())?;
        let frames = transport
            .link_poll(self.link_id, LinkSide::Joiner(&self.key), self.read)
            .map_err(|e| e.to_string())?;
        for frame in &frames.frames {
            self.read += 1;
            match self
                .join
                .on_frame(frame, self.keystore.as_ref())
                .map_err(|e| e.to_string())?
            {
                JoinStep::Compare(_) => self.state = "compare",
                JoinStep::Granted => {}
                JoinStep::Done(handed) => {
                    let handed = *handed;
                    lockdown_vault::handover::restore_from_root_key(
                        &self.paths.root,
                        APP,
                        self.keystore.as_ref(),
                        handed.irk,
                        handed.irk_id,
                        &handed.backup,
                    )
                    .map_err(|e| e.to_string())?;
                    self.secret_key = handed.secret_key;
                    self.secret_key_state = Some(handed.secret_key_state);
                    self.state = "approved";
                    return Ok(self.view());
                }
            }
        }
        if frames.closed && self.state != "approved" {
            return Err("the other device closed the link; nothing was changed here".into());
        }
        Ok(self.view())
    }

    /// The server this join is on.
    pub fn server(&self) -> &str {
        &self.server
    }

    /// The Secret Key that came, if any.
    pub fn secret_key(&self) -> Option<[u8; 16]> {
        self.secret_key
    }
}

impl Drop for Joining {
    fn drop(&mut self) {
        if let Some(sk) = self.secret_key.as_mut() {
            zeroize::Zeroize::zeroize(sk);
        }
    }
}

/// After a join was approved (the identity is here, the account not yet):
/// signs in with the account password, brings the records in step, keeps
/// the session, and keeps the Secret Key once it opened the account slot.
pub fn finish_joined(
    paths: &Paths,
    server: &str,
    login: &str,
    sign_in: SignIn<'_>,
    secret_key: Option<[u8; 16]>,
) -> Result<Vec<String>, String> {
    let keystore = Rc::new(
        FileKeystore::open_or_create(&paths.data.join(KEYSTORE_FILE)).map_err(|e| e.to_string())?,
    );
    let mut memory = HeadMemory::open(&paths.data.join(MEMORY_FILE)).map_err(|e| e.to_string())?;
    let mut identity =
        lockdown_vault::identity::open_identity(&paths.root, keystore.as_ref(), memory.identity())
            .map_err(|e| e.to_string())?
            .ok_or("nothing was joined on this device yet")?;
    let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
    let (export_key, session) = match sign_in {
        SignIn::Password(password) => {
            let mut auth = OpaqueAccountAuth::new(transport, login, binding_of(&keystore));
            let out = auth.sign_in(password).map_err(|e| e.to_string())?;
            (out.0, out.1)
        }
        SignIn::Done(export_key, session) => (export_key, session),
    };
    let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
    if session.second_factor.is_some() {
        return Err(
            "the server asks this device for a security key, though another device approved it; sign in again in a minute"
                .into(),
        );
    }
    if session.profile_id != identity.profile().profile_id {
        return Err(format!(
            "the account '{login}' belongs to another identity than the device that approved this one; nothing was kept"
        ));
    }
    memory
        .remember_identity(identity.head())
        .map_err(|e| e.to_string())?;
    let isk_pub = identity.profile().isk_pub;
    let mut synced = SyncedIdentity::open(&paths.data.join(IDENTITY_SYNC_FILE), Some(&isk_pub))
        .map_err(|e| e.to_string())?;
    sync_identity_records(
        &transport,
        &session.token,
        &mut identity,
        keystore.as_ref(),
        &mut synced,
        &mut memory,
    )?;
    let profile_id = identity.profile().profile_id;
    let mut lines = vec![format!(
        "signed in as {login} at {server}; this device ({}) joined through another of your devices",
        device_name()
    )];
    match secret_key {
        Some(sk) if secret_key::opens_account_slot(&identity, &export_key, Some(&sk)) => {
            secret_key::keep(&paths.data, keystore.as_ref(), profile_id, &sk)?;
            lines.push("your Secret Key is kept on this device too".into());
        }
        Some(_) => lines.push(
            "the Secret Key that came didn't open your account slot, so it wasn't kept".into(),
        ),
        None if secret_key::opens_account_slot(&identity, &export_key, None) => {
            secret_key::keep_opted_out(&paths.data, keystore.as_ref(), profile_id)?;
        }
        None => {}
    }
    let cfg = SyncConfig {
        v: SYNC_V,
        server: transport.base().to_string(),
        login: login.to_string(),
        profile_id: session.profile_id,
        token: session.token,
        expires_at: session.expires_at,
        vaults: BTreeMap::new(),
        last_sync_at: None,
    };
    cfg.save(&paths.sync_file())?;
    Ok(lines)
}

/// How the joined device signs in: with the password here, or with a
/// sign-in the shell already ran with this device's keystore (the web
/// vault, which needs the export key to seal its device key).
pub enum SignIn<'a> {
    /// The account password.
    Password(&'a str),
    /// The export key and session of a sign-in already made.
    Done(
        ecosystem_identity::ExportKey,
        lockdown_crypto::auth::Session,
    ),
}

impl Session {
    /// The account's transport, session token and server.
    pub(crate) fn account_online(&self) -> Result<(HttpTransport, String, String), String> {
        let cfg = self
            .sync
            .as_ref()
            .ok_or("this needs the account on this device; register or log in first")?;
        let transport = HttpTransport::new(&cfg.server).map_err(|e| e.to_string())?;
        Ok((
            transport,
            cfg.session_token()?.to_string(),
            cfg.server.clone(),
        ))
    }

    /// Makes the account with its slot opened by the password alone (the
    /// documented opt-out, 04-security §3 step 4; the App asked the person
    /// to confirm the warning first). The recovery kit is made as always:
    /// its recovery code still rebuilds the identity.
    pub fn account_register_opted_out(
        &mut self,
        server: &str,
        login: &str,
        password: &str,
    ) -> Result<crate::types::Registered, String> {
        let transport = HttpTransport::new(server).map_err(|e| e.to_string())?;
        let mut auth = OpaqueAccountAuth::new(transport, login, self.device_binding());
        let kit_text =
            if self.identity.profile().recovery == ecosystem_identity::RecoveryState::None {
                let kit = self
                    .identity
                    .create_recovery_kit()
                    .map_err(|e| e.to_string())?;
                self.memory
                    .remember_identity(self.identity.head())
                    .map_err(|e| e.to_string())?;
                Some(super::render_kit(
                    &kit,
                    Some((login, server)),
                    &device_name(),
                ))
            } else {
                None
            };
        self.identity
            .enable_account_unlock(&mut auth, password, None)
            .map_err(|e| e.to_string())?;
        self.memory
            .remember_identity(self.identity.head())
            .map_err(|e| e.to_string())?;
        secret_key::keep_opted_out(
            &self.paths.data,
            self.keystore.as_ref(),
            self.identity.profile().profile_id,
        )?;
        let (_export_key, session) = auth.sign_in(password).map_err(|e| e.to_string())?;
        sync_identity_records(
            auth.rounds(),
            &session.token,
            &mut self.identity,
            self.keystore.as_ref(),
            &mut self.synced,
            &mut self.memory,
        )?;
        let cfg = SyncConfig {
            v: SYNC_V,
            server: auth.rounds().base().to_string(),
            login: login.to_string(),
            profile_id: session.profile_id,
            token: session.token,
            expires_at: session.expires_at,
            vaults: BTreeMap::new(),
            last_sync_at: None,
        };
        cfg.save(&self.paths.sync_file())?;
        self.sync = Some(cfg);
        Ok(crate::types::Registered {
            lines: vec![
                format!("account {login} registered at {server}"),
                "without a Secret Key: your password alone opens this account on a new device"
                    .into(),
                "session kept; sync now to push the vault".into(),
            ],
            kit: kit_text.map(|t| crate::Revealed::from_text(&t)),
        })
    }
}
