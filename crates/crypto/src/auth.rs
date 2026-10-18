//! Account login with OPAQUE (RFC 9807), behind [`AccountAuth`]
//! (04-security §3).
//!
//! OPAQUE lets the client prove it knows the password without ever sending
//! the password or a salt. The server keeps only a *password file* per
//! account (the OPAQUE registration record) and its own *server setup*
//! (the OPRF seed and key pair). A stolen database gives no head start on
//! guessing without that setup; a hostile server that swaps one account's
//! password file for another's learns nothing, because the client's
//! envelope check fails before anything leaves the device.
//!
//! The protocol is written here as a pure state machine with byte
//! messages, no HTTP:
//!
//! ```text
//! registration (two round trips)              login (two round trips)
//! client                      server          client                      server
//! RegistrationClient::start → request       LoginClient::start → request
//!            response ← registration_response           response ← login_response
//! finish → upload, export_key                finish → finalization, export_key, session_key
//!            () ← password_file(upload)                 session_key ← login_finish
//! ```
//!
//! What the identity needs from a login is the 64-byte `export_key`
//! ([`ExportKey`]): it mixes with the Secret Key into the Account Unlock
//! Key (`ecosystem_identity::kdf::account_unlock_key`) and that opens the
//! `account` wrap of the IRK. Nothing else is derived from it here.
//!
//! The cipher suite is [`Suite`]: ristretto255 for the OPRF and the key
//! exchange (3DH), SHA-512, and Argon2id as the key-stretching function at
//! Lockdown's desktop floor ([`ACCOUNT_KSF`]). The stretching step runs
//! on the client over the OPRF output, so the "salt" is the server-held
//! OPRF key and no salt is ever exposed (04-security §3 step 2).
//!
//! [`OpaqueAccountAuth`] drives the client half over any [`AuthRounds`]
//! (the HTTP one lives in `lockdown-sync`), and the identity crate's
//! [`LocalStubAuth`] stays the stand-in for a vault with no server.

use std::fmt;

pub use ecosystem_identity::auth::{AccountAuth, ExportKey, LocalStubAuth};
use ecosystem_identity::kdf::{Argon2Params, FLOOR_DESKTOP, FLOOR_TEST};
use ecosystem_identity::records::Id;
use opaque_ke::errors::{InternalError, ProtocolError};
use opaque_ke::generic_array::{ArrayLength, GenericArray};
use opaque_ke::ksf::Ksf;
use opaque_ke::rand::{CryptoRng, RngCore};
use opaque_ke::{
    CipherSuite, ClientLogin, ClientLoginFinishParameters, ClientRegistration,
    ClientRegistrationFinishParameters, CredentialFinalization, CredentialRequest,
    CredentialResponse, RegistrationRequest, RegistrationResponse, RegistrationUpload,
    Ristretto255, ServerLogin, ServerLoginParameters, ServerRegistration, ServerSetup, TripleDh,
};
use zeroize::Zeroize;

use crate::secret::SecretBytes;
use crate::{Error, Result};

// --- the suite ------------------------------------------------------------

/// Lockdown's OPAQUE cipher suite: ristretto255 OPRF, 3DH over
/// ristretto255 with SHA-512, Argon2id stretching ([`AccountKsf`]).
pub struct Suite;

impl CipherSuite for Suite {
    type OprfCs = Ristretto255;
    type KeyExchange = TripleDh<Ristretto255, sha2_010::Sha512>;
    type Ksf = AccountKsf;
}

/// The Argon2id parameters OPAQUE's stretching step uses: the desktop floor
/// (04-security §4), fixed by the suite rather than negotiated, because a
/// login must produce the same key on every device of the account and
/// OPAQUE carries no signed parameter record. Under the `insecure-test-kdf`
/// feature it is the identity crate's test floor, so debug tests finish in
/// milliseconds; never enable that feature in an app.
pub const ACCOUNT_KSF: Argon2Params = if cfg!(feature = "insecure-test-kdf") {
    FLOOR_TEST
} else {
    FLOOR_DESKTOP
};

/// The most an account's stretching may cost, whatever a server announces
/// (D249): a gibibyte of memory, sixteen passes, sixteen lanes, the same
/// ceiling a password-sealed export reads under (D139). A server that names
/// more is refused before anything is computed, so it cannot pin a device.
pub const KSF_CEILING: Argon2Params = Argon2Params {
    m_cost_kib: 1024 * 1024,
    t_cost: 16,
    p_cost: 16,
};

/// Whether `params` may stretch an account password on this build: at or
/// above [`ACCOUNT_KSF`] in every parameter, and within [`KSF_CEILING`].
pub fn ksf_acceptable(params: &Argon2Params) -> bool {
    params.meets(&ACCOUNT_KSF)
        && params.m_cost_kib <= KSF_CEILING.m_cost_kib
        && params.t_cost <= KSF_CEILING.t_cost
        && params.p_cost <= KSF_CEILING.p_cost
}

/// Argon2id as OPAQUE's key-stretching function, at [`ACCOUNT_KSF`] by
/// default or at an account's raised parameters (`client-kdf`, D249). The
/// input is the OPRF output (already high-entropy and bound to the
/// server's OPRF key), so the salt carries nothing and is fixed, as the
/// `opaque-ke` crate's own Argon2 binding does.
#[derive(Debug, Clone, Copy)]
pub struct AccountKsf {
    params: Argon2Params,
}

impl Default for AccountKsf {
    fn default() -> Self {
        Self {
            params: ACCOUNT_KSF,
        }
    }
}

impl AccountKsf {
    /// The stretching at `params`; refused below [`ACCOUNT_KSF`] or above
    /// [`KSF_CEILING`].
    pub fn at(params: Argon2Params) -> Result<Self> {
        if !ksf_acceptable(&params) {
            return Err(Error::Invalid(format!(
                "an account's password stretching must be between {ACCOUNT_KSF} and {KSF_CEILING}; {params} isn't"
            )));
        }
        Ok(Self { params })
    }

    /// The parameters in force.
    pub fn params(&self) -> Argon2Params {
        self.params
    }
}

const KSF_SALT: [u8; 16] = [0; 16];

impl Ksf for AccountKsf {
    fn hash<L: ArrayLength<u8>>(
        &self,
        input: GenericArray<u8, L>,
    ) -> std::result::Result<GenericArray<u8, L>, InternalError> {
        let params = argon2::Params::new(
            self.params.m_cost_kib,
            self.params.t_cost,
            self.params.p_cost,
            Some(L::USIZE),
        )
        .map_err(|_| InternalError::KsfError)?;
        let argon =
            argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let mut output = GenericArray::<u8, L>::default();
        argon
            .hash_password_into(&input, &KSF_SALT, &mut output)
            .map_err(|_| InternalError::KsfError)?;
        Ok(output)
    }
}

/// The operating system's generator, as the random source `opaque-ke`
/// asks for. Callers check [`crate::rand::random_32`] first so a failing
/// source is reported as [`Error::Random`] before any protocol step.
struct OsRng;

impl RngCore for OsRng {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        self.fill_bytes(&mut b);
        u32::from_le_bytes(b)
    }

    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        if crate::rand::random_bytes(dest).is_err() {
            // The same behaviour as every OS-backed generator: there is no
            // safe value to hand back. The caller probed the source first.
            panic!("the operating system gave no random bytes");
        }
    }

    fn try_fill_bytes(
        &mut self,
        dest: &mut [u8],
    ) -> std::result::Result<(), opaque_ke::rand::Error> {
        crate::rand::random_bytes(dest).map_err(|_| {
            opaque_ke::rand::Error::from(
                std::num::NonZeroU32::new(opaque_ke::rand::Error::CUSTOM_START)
                    .expect("CUSTOM_START is non-zero"),
            )
        })
    }
}

impl CryptoRng for OsRng {}

fn rng() -> Result<OsRng> {
    crate::rand::random_32()?;
    Ok(OsRng)
}

fn protocol(e: ProtocolError) -> Error {
    match e {
        ProtocolError::InvalidLoginError => Error::LoginFailed,
        ProtocolError::SerializationError | ProtocolError::SizeError { .. } => {
            Error::Invalid("an OPAQUE message didn't decode".into())
        }
        ProtocolError::ReflectedValueError => {
            Error::Invalid("the server reflected the blinded password; refused".into())
        }
        ProtocolError::LibraryError(_) | ProtocolError::Custom(_) => {
            Error::Invalid("an OPAQUE step failed".into())
        }
    }
}

fn export_key_from(mut out: GenericArray<u8, opaque_ke::generic_array::typenum::U64>) -> ExportKey {
    let mut bytes = [0u8; 64];
    bytes.copy_from_slice(&out);
    out.as_mut_slice().zeroize();
    let key = ExportKey::from_bytes(bytes);
    bytes.zeroize();
    key
}

// --- client ---------------------------------------------------------------

/// The client half of registration, between the two round trips.
pub struct RegistrationClient {
    state: ClientRegistration<Suite>,
}

impl fmt::Debug for RegistrationClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RegistrationClient(…)")
    }
}

impl RegistrationClient {
    /// Blinds the password. Returns the state to keep and the request to
    /// send.
    pub fn start(password: &str) -> Result<(Self, Vec<u8>)> {
        let mut rng = rng()?;
        let started =
            ClientRegistration::<Suite>::start(&mut rng, password.as_bytes()).map_err(protocol)?;
        Ok((
            Self {
                state: started.state,
            },
            started.message.serialize().to_vec(),
        ))
    }

    /// Finishes with the server's response. Returns the upload to send
    /// (about as sensitive as a password hash: send it only over TLS) and
    /// the export key.
    pub fn finish(self, password: &str, response: &[u8]) -> Result<(Vec<u8>, ExportKey)> {
        self.finish_stretched(password, response, &AccountKsf::default())
    }

    /// [`Self::finish`] with the stretching at `ksf`: the parameters the
    /// account's password file will be made under (a renegotiation, D249).
    pub fn finish_stretched(
        self,
        password: &str,
        response: &[u8],
        ksf: &AccountKsf,
    ) -> Result<(Vec<u8>, ExportKey)> {
        let mut rng = rng()?;
        let response = RegistrationResponse::<Suite>::deserialize(response).map_err(protocol)?;
        let finished = self
            .state
            .finish(
                &mut rng,
                password.as_bytes(),
                response,
                ClientRegistrationFinishParameters::new(Default::default(), Some(ksf)),
            )
            .map_err(protocol)?;
        Ok((
            finished.message.serialize().to_vec(),
            export_key_from(finished.export_key),
        ))
    }

    /// The state as bytes, for a client that cannot hold it in memory
    /// between round trips. Treat it like the password.
    pub fn to_bytes(&self) -> SecretBytes {
        SecretBytes::new(self.state.serialize().to_vec())
    }

    /// From [`Self::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(Self {
            state: ClientRegistration::<Suite>::deserialize(bytes).map_err(protocol)?,
        })
    }
}

/// What a finished login hands the client.
pub struct LoginFinished {
    /// The finalization message to send; the server's own check.
    pub finalization: Vec<u8>,
    /// The export key the identity derives the Account Unlock Key from.
    pub export_key: ExportKey,
    /// The shared session key (64 bytes), the same on both sides.
    pub session_key: SecretBytes,
}

impl fmt::Debug for LoginFinished {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LoginFinished(…)")
    }
}

/// The client half of login, between the two round trips.
pub struct LoginClient {
    state: ClientLogin<Suite>,
}

impl fmt::Debug for LoginClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LoginClient(…)")
    }
}

impl LoginClient {
    /// Blinds the password and starts the key exchange.
    pub fn start(password: &str) -> Result<(Self, Vec<u8>)> {
        let mut rng = rng()?;
        let started =
            ClientLogin::<Suite>::start(&mut rng, password.as_bytes()).map_err(protocol)?;
        Ok((
            Self {
                state: started.state,
            },
            started.message.serialize().to_vec(),
        ))
    }

    /// Finishes with the server's response. A wrong password, an unknown
    /// login (the server answers with a decoy) or a swapped password file
    /// all end here as [`Error::LoginFailed`], before anything is sent.
    pub fn finish(self, password: &str, response: &[u8]) -> Result<LoginFinished> {
        self.finish_stretched(password, response, &AccountKsf::default())
    }

    /// [`Self::finish`] with the stretching at `ksf`, the parameters the
    /// account's password file was made under (announced by the server and
    /// checked with [`ksf_acceptable`] first).
    pub fn finish_stretched(
        self,
        password: &str,
        response: &[u8],
        ksf: &AccountKsf,
    ) -> Result<LoginFinished> {
        let mut rng = rng()?;
        let response = CredentialResponse::<Suite>::deserialize(response).map_err(protocol)?;
        let finished = self
            .state
            .finish(
                &mut rng,
                password.as_bytes(),
                response,
                ClientLoginFinishParameters::new(None, Default::default(), Some(ksf)),
            )
            .map_err(protocol)?;
        let mut session = finished.session_key;
        let session_key = SecretBytes::new(session.to_vec());
        session.as_mut_slice().zeroize();
        Ok(LoginFinished {
            finalization: finished.message.serialize().to_vec(),
            export_key: export_key_from(finished.export_key),
            session_key,
        })
    }

    /// The state as bytes, between round trips. Treat it like the password.
    pub fn to_bytes(&self) -> SecretBytes {
        SecretBytes::new(self.state.serialize().to_vec())
    }

    /// From [`Self::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(Self {
            state: ClientLogin::<Suite>::deserialize(bytes).map_err(protocol)?,
        })
    }
}

// --- server ---------------------------------------------------------------

/// The server's OPAQUE setup: the OPRF seed and the server key pair. Made
/// once, kept secret, and never rotated without re-registering every
/// account (the password files are bound to it).
pub struct OpaqueServer {
    setup: ServerSetup<Suite>,
}

impl fmt::Debug for OpaqueServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OpaqueServer(…)")
    }
}

/// The server's half of a login between the two round trips. Keep it for
/// the finish; it is useless afterwards.
pub struct LoginServerState(Vec<u8>);

impl LoginServerState {
    /// The state as bytes.
    pub fn to_bytes(&self) -> &[u8] {
        &self.0
    }

    /// From bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
}

impl fmt::Debug for LoginServerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LoginServerState(…)")
    }
}

impl OpaqueServer {
    /// A fresh setup.
    pub fn generate() -> Result<Self> {
        let mut rng = rng()?;
        Ok(Self {
            setup: ServerSetup::<Suite>::new(&mut rng),
        })
    }

    /// The setup as bytes, to keep in the server's key file.
    pub fn to_bytes(&self) -> SecretBytes {
        SecretBytes::new(self.setup.serialize().to_vec())
    }

    /// From [`Self::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(Self {
            setup: ServerSetup::<Suite>::deserialize(bytes).map_err(protocol)?,
        })
    }

    /// Answers a registration request for `credential_id` (the login name
    /// the account is filed under). Stateless: the finish only needs the
    /// upload.
    pub fn registration_response(&self, credential_id: &[u8], request: &[u8]) -> Result<Vec<u8>> {
        let request = RegistrationRequest::<Suite>::deserialize(request).map_err(protocol)?;
        let started = ServerRegistration::<Suite>::start(&self.setup, request, credential_id)
            .map_err(protocol)?;
        Ok(started.message.serialize().to_vec())
    }

    /// Turns the client's upload into the password file to store for the
    /// account. Refuses an upload that does not decode.
    pub fn password_file(upload: &[u8]) -> Result<Vec<u8>> {
        let upload = RegistrationUpload::<Suite>::deserialize(upload).map_err(protocol)?;
        Ok(ServerRegistration::<Suite>::finish(upload)
            .serialize()
            .to_vec())
    }

    /// Answers a login request. With no password file (an unknown login)
    /// the answer is a decoy that fails on the client exactly like a wrong
    /// password, so the request learns nothing. Returns the response and
    /// the state to keep for [`Self::login_finish`].
    pub fn login_response(
        &self,
        credential_id: &[u8],
        password_file: Option<&[u8]>,
        request: &[u8],
    ) -> Result<(Vec<u8>, LoginServerState)> {
        let mut rng = rng()?;
        let request = CredentialRequest::<Suite>::deserialize(request).map_err(protocol)?;
        let file = match password_file {
            Some(bytes) => Some(ServerRegistration::<Suite>::deserialize(bytes).map_err(protocol)?),
            None => None,
        };
        let started = ServerLogin::<Suite>::start(
            &mut rng,
            &self.setup,
            file,
            request,
            credential_id,
            ServerLoginParameters::default(),
        )
        .map_err(protocol)?;
        Ok((
            started.message.serialize().to_vec(),
            LoginServerState(started.state.serialize().to_vec()),
        ))
    }

    /// Checks the client's finalization and returns the session key (64
    /// bytes). A forged or replayed message is [`Error::LoginFailed`].
    pub fn login_finish(state: &LoginServerState, finalization: &[u8]) -> Result<SecretBytes> {
        let state = ServerLogin::<Suite>::deserialize(&state.0).map_err(protocol)?;
        let message =
            CredentialFinalization::<Suite>::deserialize(finalization).map_err(protocol)?;
        let finished = state
            .finish(message, ServerLoginParameters::default())
            .map_err(protocol)?;
        let mut key = finished.session_key;
        let out = SecretBytes::new(key.to_vec());
        key.as_mut_slice().zeroize();
        Ok(out)
    }
}

// --- the account client over any transport ---------------------------------

/// A device's claim at login: which device this is and proof it holds the
/// key, so the server can later require that a vault head was signed by a
/// device the account's owner logged in with.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DeviceProof {
    /// The device's id (the identity's `Device.device_id`).
    pub device_id: Id,
    /// Its Ed25519 public key (`Device.device_pub`).
    pub device_pub: [u8; 32],
    /// Ed25519 over [`device_pop_message`] with the device key.
    pub pop: Vec<u8>,
}

/// The bytes a device signs to prove it holds its key at login: a label,
/// the server's one-time login id, and the login name. The login id is
/// fresh per attempt, so the proof cannot be replayed.
pub fn device_pop_message(login: &str, login_id: &Id) -> Vec<u8> {
    let mut m = b"lockdown/device-pop/v1".to_vec();
    m.extend_from_slice(login_id.as_bytes());
    m.extend_from_slice(login.as_bytes());
    m
}

/// Signs with the device key: the device keystore on a phone, a key in
/// memory on a desktop.
pub type SignFn = Box<dyn Fn(&[u8]) -> Result<[u8; 64]>>;

/// How this device signs: its id, its public key, and a way to sign with
/// the private half.
pub struct DeviceBinding {
    /// The device's id.
    pub device_id: Id,
    /// Its Ed25519 public key.
    pub device_pub: [u8; 32],
    /// Signs with the device key.
    pub sign: SignFn,
}

impl fmt::Debug for DeviceBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceBinding")
            .field("device_id", &self.device_id)
            .finish_non_exhaustive()
    }
}

/// The server's answer to the first login round trip.
#[derive(Clone, Debug)]
pub struct LoginStarted {
    /// The server's one-time id for this attempt.
    pub login_id: Id,
    /// The OPAQUE credential response.
    pub response: Vec<u8>,
    /// The stretching the account's password file was made under, when the
    /// server announced one (absent: [`ACCOUNT_KSF`], the suite's default).
    pub ksf: Option<Argon2Params>,
}

/// The stretching to finish `started` under: what the server announced
/// (the suite's default when it named none), refused when outside
/// [`ksf_acceptable`] or weaker in any parameter than `at_least`, the
/// stretching this device used for the account before.
pub fn ksf_for(started: &LoginStarted, at_least: Option<&Argon2Params>) -> Result<AccountKsf> {
    let params = started.ksf.unwrap_or(ACCOUNT_KSF);
    if let Some(floor) = at_least
        && !params.meets(floor)
    {
        return Err(Error::Invalid(format!(
            "the server asked for weaker password stretching ({params}) than this device used for the account before ({floor}); nothing was sent"
        )));
    }
    AccountKsf::at(params)
}

/// What a finished login leaves the client with besides the export key.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Session {
    /// The bearer token for the authenticated endpoints. Empty while
    /// [`Session::second_factor`] is set: the password checked out, and the
    /// account wants a security key before it hands out a session.
    pub token: String,
    /// When the token stops working (seconds since the Unix epoch).
    pub expires_at: i64,
    /// The profile the account was registered for.
    pub profile_id: Id,
    /// The account's second factor, when this sign-in needs one
    /// (`account-security-keys`, D250).
    pub second_factor: Option<SecondFactorNeeded>,
}

/// A sign-in that passed the password and waits for a security key or a
/// passkey (WebAuthn) before the server issues the session.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SecondFactorNeeded {
    /// The server's id for the waiting sign-in.
    pub pending_id: Id,
    /// The secret only this device holds, to collect the session once the
    /// key was used (in this window or in the system browser).
    pub poll_key: Vec<u8>,
    /// What the browser's WebAuthn `get` needs, as the server wrote it
    /// (CBOR; `lockdown_sync::account_security::SecondFactorRequest`).
    pub request: Vec<u8>,
}

/// The four round trips, over whatever carries them. `lockdown-sync`
/// implements this over HTTPS; tests implement it in process.
pub trait AuthRounds {
    /// Registration, first round trip: the blinded request in, the
    /// response out.
    fn register_start(&mut self, login: &str, request: &[u8]) -> Result<Vec<u8>>;
    /// Registration, second round trip: the upload in; the account exists
    /// afterwards.
    fn register_finish(&mut self, login: &str, profile_id: &Id, upload: &[u8]) -> Result<()>;
    /// Login, first round trip.
    fn login_start(&mut self, login: &str, request: &[u8]) -> Result<LoginStarted>;
    /// Login, second round trip: the finalization and the device's proof
    /// in, a session out.
    fn login_finish(
        &mut self,
        login_id: &Id,
        finalization: &[u8],
        device: &DeviceProof,
    ) -> Result<Session>;
}

impl<R: AuthRounds + ?Sized> AuthRounds for &mut R {
    fn register_start(&mut self, login: &str, request: &[u8]) -> Result<Vec<u8>> {
        (**self).register_start(login, request)
    }

    fn register_finish(&mut self, login: &str, profile_id: &Id, upload: &[u8]) -> Result<()> {
        (**self).register_finish(login, profile_id, upload)
    }

    fn login_start(&mut self, login: &str, request: &[u8]) -> Result<LoginStarted> {
        (**self).login_start(login, request)
    }

    fn login_finish(
        &mut self,
        login_id: &Id,
        finalization: &[u8],
        device: &DeviceProof,
    ) -> Result<Session> {
        (**self).login_finish(login_id, finalization, device)
    }
}

/// The OPAQUE account client: [`AccountAuth`] for the identity crate, over
/// any [`AuthRounds`]. Holds the login name the account is filed under and
/// this device's binding; keeps the last [`Session`].
pub struct OpaqueAccountAuth<R: AuthRounds> {
    rounds: R,
    login: String,
    device: DeviceBinding,
    session: Option<Session>,
    /// The weakest stretching this device will sign in under for this
    /// account: what it used last (parameters only rise, D249).
    at_least: Option<Argon2Params>,
    /// What the last sign-in stretched under.
    last_ksf: Option<Argon2Params>,
}

impl<R: AuthRounds> fmt::Debug for OpaqueAccountAuth<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpaqueAccountAuth")
            .field("login", &self.login)
            .field("device", &self.device)
            .field("signed_in", &self.session.is_some())
            .finish()
    }
}

impl<R: AuthRounds> OpaqueAccountAuth<R> {
    /// A client for the account filed under `login`, on this device.
    pub fn new(rounds: R, login: impl Into<String>, device: DeviceBinding) -> Self {
        Self {
            rounds,
            login: login.into(),
            device,
            session: None,
            at_least: None,
            last_ksf: None,
        }
    }

    /// Refuses to sign in under weaker stretching than `params`: what this
    /// device used for the account before (a server that announces less is
    /// refused before anything is computed).
    pub fn at_least(mut self, params: Option<Argon2Params>) -> Self {
        self.at_least = params;
        self
    }

    /// The stretching the last successful sign-in used.
    pub fn last_ksf(&self) -> Option<Argon2Params> {
        self.last_ksf
    }

    /// The login name.
    pub fn login_name(&self) -> &str {
        &self.login
    }

    /// The session from the last successful [`Self::sign_in`].
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// The transport, to reuse for the authenticated calls.
    pub fn rounds(&self) -> &R {
        &self.rounds
    }

    /// The transport, mutably.
    pub fn rounds_mut(&mut self) -> &mut R {
        &mut self.rounds
    }

    /// Takes the transport back, to carry on with the authenticated calls.
    pub fn into_rounds(self) -> R {
        self.rounds
    }

    /// Registers the account: two round trips, and the export key out.
    pub fn register_account(&mut self, profile_id: &Id, password: &str) -> Result<ExportKey> {
        let (client, request) = RegistrationClient::start(password)?;
        let response = self.rounds.register_start(&self.login, &request)?;
        let (upload, export_key) = client.finish(password, &response)?;
        self.rounds
            .register_finish(&self.login, profile_id, &upload)?;
        Ok(export_key)
    }

    /// Logs in without knowing the profile yet (a new device): two round
    /// trips, the export key and the session out.
    pub fn sign_in(&mut self, password: &str) -> Result<(ExportKey, Session)> {
        let (client, request) = LoginClient::start(password)?;
        let started = self.rounds.login_start(&self.login, &request)?;
        let ksf = ksf_for(&started, self.at_least.as_ref())?;
        let finished = client.finish_stretched(password, &started.response, &ksf)?;
        self.last_ksf = Some(ksf.params());
        let pop = (self.device.sign)(&device_pop_message(&self.login, &started.login_id))?;
        let proof = DeviceProof {
            device_id: self.device.device_id,
            device_pub: self.device.device_pub,
            pop: pop.to_vec(),
        };
        let session =
            self.rounds
                .login_finish(&started.login_id, &finished.finalization, &proof)?;
        self.session = Some(session.clone());
        Ok((finished.export_key, session))
    }
}

fn to_identity(e: Error) -> ecosystem_identity::Error {
    match e {
        Error::Identity(inner) => inner,
        Error::Unsupported(m) => ecosystem_identity::Error::Unsupported(m),
        Error::Random => ecosystem_identity::Error::Random,
        other => ecosystem_identity::Error::Invalid(other.to_string()),
    }
}

impl<R: AuthRounds> AccountAuth for OpaqueAccountAuth<R> {
    fn register(
        &mut self,
        profile_id: &Id,
        password: &str,
    ) -> ecosystem_identity::Result<ExportKey> {
        if password.chars().count() < 8 {
            return Err(ecosystem_identity::Error::Invalid(
                "use a password of at least 8 characters".into(),
            ));
        }
        self.register_account(profile_id, password)
            .map_err(to_identity)
    }

    fn login(&mut self, profile_id: &Id, password: &str) -> ecosystem_identity::Result<ExportKey> {
        let (export_key, session) = self.sign_in(password).map_err(to_identity)?;
        if session.profile_id != *profile_id {
            return Err(ecosystem_identity::Error::Invalid(format!(
                "the account '{}' belongs to another profile ({}), not {profile_id}",
                self.login, session.profile_id
            )));
        }
        Ok(export_key)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::SigningKey;

    /// The server's bookkeeping, in process: what `lockdown-server` does
    /// over HTTP, without the HTTP.
    struct InProcessServer {
        server: OpaqueServer,
        accounts: BTreeMap<String, (Id, Vec<u8>)>,
        pending: BTreeMap<Id, (String, LoginServerState)>,
        issued: Vec<DeviceProof>,
    }

    impl InProcessServer {
        fn new() -> Self {
            Self {
                server: OpaqueServer::generate().unwrap(),
                accounts: BTreeMap::new(),
                pending: BTreeMap::new(),
                issued: Vec::new(),
            }
        }
    }

    impl AuthRounds for InProcessServer {
        fn register_start(&mut self, login: &str, request: &[u8]) -> Result<Vec<u8>> {
            if self.accounts.contains_key(login) {
                return Err(Error::Invalid("that login name is taken".into()));
            }
            self.server.registration_response(login.as_bytes(), request)
        }

        fn register_finish(&mut self, login: &str, profile_id: &Id, upload: &[u8]) -> Result<()> {
            let file = OpaqueServer::password_file(upload)?;
            self.accounts.insert(login.to_string(), (*profile_id, file));
            Ok(())
        }

        fn login_start(&mut self, login: &str, request: &[u8]) -> Result<LoginStarted> {
            let file = self.accounts.get(login).map(|(_, f)| f.as_slice());
            let (response, state) = self
                .server
                .login_response(login.as_bytes(), file, request)?;
            let login_id = Id::new();
            self.pending.insert(login_id, (login.to_string(), state));
            Ok(LoginStarted {
                login_id,
                response,
                ksf: None,
            })
        }

        fn login_finish(
            &mut self,
            login_id: &Id,
            finalization: &[u8],
            device: &DeviceProof,
        ) -> Result<Session> {
            let (login, state) = self.pending.remove(login_id).ok_or(Error::LoginFailed)?;
            OpaqueServer::login_finish(&state, finalization)?;
            let key = crate::VerifyingKey::from_bytes(&device.device_pub)?;
            key.verify(
                &device_pop_message(&login, login_id),
                &device.pop,
                "device proof",
            )?;
            self.issued.push(device.clone());
            let (profile_id, _) = self.accounts.get(&login).ok_or(Error::LoginFailed)?;
            Ok(Session {
                token: format!("token-for-{login}"),
                expires_at: i64::MAX,
                profile_id: *profile_id,
                second_factor: None,
            })
        }
    }

    fn device() -> DeviceBinding {
        let key = SigningKey::generate().unwrap();
        DeviceBinding {
            device_id: Id::new(),
            device_pub: key.verifying_key().to_bytes(),
            sign: Box::new(move |m| Ok(key.sign(m))),
        }
    }

    #[test]
    fn register_then_login_gives_the_same_export_key_and_a_session() {
        let mut auth = OpaqueAccountAuth::new(InProcessServer::new(), "me@example.com", device());
        let profile = Id::new();
        let registered = auth
            .register(&profile, "correct horse battery staple")
            .unwrap();
        let logged_in = auth
            .login(&profile, "correct horse battery staple")
            .unwrap();
        assert_eq!(registered.expose(), logged_in.expose());
        let session = auth.session().unwrap();
        assert_eq!(session.profile_id, profile);
        assert_eq!(session.token, "token-for-me@example.com");
        // The device proved it held its key.
        assert_eq!(auth.rounds().issued.len(), 1);
        assert_eq!(auth.rounds().issued[0].device_id, auth.device.device_id);
        // Short passwords are refused before any round trip.
        assert!(matches!(
            auth.register(&Id::new(), "short"),
            Err(ecosystem_identity::Error::Invalid(_))
        ));
    }

    #[test]
    fn a_wrong_password_and_an_unknown_login_fail_the_same_way() {
        let mut server = InProcessServer::new();
        let profile = Id::new();
        let mut auth = OpaqueAccountAuth::new(&mut server, "me@example.com", device());
        auth.register(&profile, "correct horse battery staple")
            .unwrap();
        let wrong = auth.sign_in("wrong horse").unwrap_err().to_string();
        // Nothing was sent for the second round trip: no pending login was
        // consumed and no session issued.
        assert_eq!(auth.rounds().pending.len(), 1);
        assert!(auth.rounds().issued.is_empty());
        assert!(auth.session().is_none());

        let mut nobody = OpaqueAccountAuth::new(&mut server, "nobody@example.com", device());
        let unknown = nobody.sign_in("correct horse battery staple").unwrap_err();
        assert_eq!(unknown.to_string(), wrong);
        assert!(matches!(unknown, Error::LoginFailed));
        // The decoy answered, so the server still cannot tell the two
        // apart by its own behaviour either: both left a pending login.
        assert_eq!(nobody.rounds().pending.len(), 2);
    }

    #[test]
    fn a_server_that_swaps_password_files_gets_nothing() {
        let mut server = InProcessServer::new();
        let (a, b) = (Id::new(), Id::new());
        OpaqueAccountAuth::new(&mut server, "a@example.com", device())
            .register(&a, "password for a, long")
            .unwrap();
        OpaqueAccountAuth::new(&mut server, "b@example.com", device())
            .register(&b, "password for b, long")
            .unwrap();
        // The hostile server files b's record under a's name.
        let (_, file_b) = server.accounts["b@example.com"].clone();
        server.accounts.get_mut("a@example.com").unwrap().1 = file_b;
        let mut auth = OpaqueAccountAuth::new(&mut server, "a@example.com", device());
        assert!(matches!(
            auth.sign_in("password for a, long"),
            Err(Error::LoginFailed)
        ));
        // a's client stopped before the finalization: the server holds no
        // session key, no export key, and nothing derived from a's
        // password beyond the blinded request it already had.
        assert!(auth.rounds().issued.is_empty());
        // And b's password does not open a's account either: the OPRF key
        // is per login name, so the file is bound to the name it was made
        // under.
        assert!(matches!(
            auth.sign_in("password for b, long"),
            Err(Error::LoginFailed)
        ));
    }

    #[test]
    fn raised_stretching_is_announced_used_and_never_lowered() {
        let server = OpaqueServer::generate().unwrap();
        let login = b"me@example.com";
        let raised = Argon2Params {
            t_cost: ACCOUNT_KSF.t_cost + 2,
            ..ACCOUNT_KSF
        };
        let ksf = AccountKsf::at(raised).unwrap();
        let (client, request) = RegistrationClient::start("a long password").unwrap();
        let response = server.registration_response(login, &request).unwrap();
        let (upload, at_registration) = client
            .finish_stretched("a long password", &response, &ksf)
            .unwrap();
        let file = OpaqueServer::password_file(&upload).unwrap();

        let login_with = |params: Option<Argon2Params>, at_least: Option<&Argon2Params>| {
            let (client, request) = LoginClient::start("a long password").unwrap();
            let (response, _) = server.login_response(login, Some(&file), &request).unwrap();
            let started = LoginStarted {
                login_id: Id::new(),
                response,
                ksf: params,
            };
            let ksf = ksf_for(&started, at_least)?;
            client.finish_stretched("a long password", &started.response, &ksf)
        };
        // The file was made under the raised set: only that set opens it.
        let finished = login_with(Some(raised), None).unwrap();
        assert_eq!(finished.export_key.expose(), at_registration.expose());
        assert!(matches!(login_with(None, None), Err(Error::LoginFailed)));
        // A server announcing less than this device used before is refused
        // before anything is stretched.
        let lowered = login_with(None, Some(&raised)).unwrap_err().to_string();
        assert!(lowered.contains("weaker password stretching"), "{lowered}");
        // Below the floor or past the ceiling: refused by name.
        let below = Argon2Params {
            m_cost_kib: 8,
            ..ACCOUNT_KSF
        };
        assert!(AccountKsf::at(below).is_err());
        let huge = Argon2Params {
            m_cost_kib: KSF_CEILING.m_cost_kib + 1,
            ..ACCOUNT_KSF
        };
        assert!(AccountKsf::at(huge).is_err());
        assert!(!ksf_acceptable(&huge) && ksf_acceptable(&ACCOUNT_KSF));
    }

    #[test]
    fn states_and_setup_round_trip_through_bytes() {
        let server = OpaqueServer::generate().unwrap();
        let server = OpaqueServer::from_bytes(server.to_bytes().expose()).unwrap();
        let login = b"me@example.com";

        let (client, request) = RegistrationClient::start("a long password").unwrap();
        let client = RegistrationClient::from_bytes(client.to_bytes().expose()).unwrap();
        let response = server.registration_response(login, &request).unwrap();
        let (upload, export_at_registration) = client.finish("a long password", &response).unwrap();
        let file = OpaqueServer::password_file(&upload).unwrap();

        let (client, request) = LoginClient::start("a long password").unwrap();
        let client = LoginClient::from_bytes(client.to_bytes().expose()).unwrap();
        let (response, state) = server.login_response(login, Some(&file), &request).unwrap();
        let state = LoginServerState::from_bytes(state.to_bytes());
        let finished = client.finish("a long password", &response).unwrap();
        let server_session = OpaqueServer::login_finish(&state, &finished.finalization).unwrap();
        assert_eq!(server_session.expose(), finished.session_key.expose());
        assert_eq!(
            finished.export_key.expose(),
            export_at_registration.expose()
        );
        assert_eq!(server_session.len(), 64);

        // A forged finalization is refused.
        let mut forged = finished.finalization.clone();
        forged[0] ^= 1;
        assert!(matches!(
            OpaqueServer::login_finish(&state, &forged),
            Err(Error::LoginFailed)
        ));
        // Garbage in any slot is an error, never a panic.
        assert!(OpaqueServer::from_bytes(b"nope").is_err());
        assert!(RegistrationClient::from_bytes(b"nope").is_err());
        assert!(LoginClient::from_bytes(b"nope").is_err());
        assert!(OpaqueServer::password_file(b"nope").is_err());
        assert!(server.registration_response(login, b"nope").is_err());
        assert!(server.login_response(login, None, b"nope").is_err());
    }

    #[test]
    fn the_ksf_is_argon2id_at_the_floor_and_deterministic() {
        let input = GenericArray::<u8, opaque_ke::generic_array::typenum::U64>::default();
        let a = AccountKsf::default().hash(input).unwrap();
        let b = AccountKsf::default().hash(input).unwrap();
        assert_eq!(a, b);
        assert_ne!(a.as_slice(), &[0u8; 64][..]);
        assert_eq!(ACCOUNT_KSF, FLOOR_TEST, "tests run at the test floor");
    }

    #[test]
    fn the_local_stub_still_works_through_the_same_trait() {
        let mut stub = LocalStubAuth::generate().unwrap();
        let id = Id::new();
        let a = stub.register(&id, "correct horse battery").unwrap();
        let b = stub.login(&id, "correct horse battery").unwrap();
        assert_eq!(a.expose(), b.expose());
    }
}
