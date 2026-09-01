//! The local identity — a **keypair**, unlocked by a passphrase.
//!
//! ★★★ `bg.myc` used to be a string the host declared. This makes it a handle
//! for a key that can **prove it is itself**, and that is the difference
//! between a name and an identity. Everything in this file exists to make one
//! sentence true: *nothing acts until the private key has been recovered from
//! disk by a passphrase that genuinely decrypts it.*
//!
//! ## ★★★ Why a keypair and not a password check
//!
//! A password verifier answers *did you type the right thing* — a boolean, on
//! one machine, meaningful to nobody else. A keypair answers *can you produce a
//! signature only this identity can produce*, which is the same question a peer
//! will ask when there is a network (slice 6). Building the verifier first and
//! the key later would mean building the wrong half twice.
//!
//! ★★ It also means the unlock **cannot be faked**. There is no stored "is the
//! passphrase right" flag to compare against and no branch to skip: a wrong
//! passphrase derives a wrong key, the AEAD's authentication tag fails, and
//! there is no private key at the end of it. Failing closed is not a policy
//! here, it is the only thing that can happen.
//!
//! ## The file
//!
//! ```text
//! identity.json
//!   handle          the name a person uses            bg.myc
//!   public_key      32 bytes, hex — never secret
//!   kdf             pbkdf2-hmac-sha256, salt, iterations
//!   sealed_secret   the private key, XChaCha20-Poly1305, hex
//!   proof           a signature over PROOF_MESSAGE by that key
//! ```
//!
//! ★ `proof` is what lets an unlock be **verified rather than assumed**: after
//! decrypting, the recovered key signs the same message and the signature is
//! checked against the stored public key. Decryption succeeding is already
//! strong (the AEAD tag), but a check against the *public* half is what makes
//! the identity's own claim about itself testable — including by anything that
//! only ever holds the public file.
//!
//! ## ★ What is honestly NOT here
//!
//! **The KDF is PBKDF2-HMAC-SHA256**, matching the Python reference, at
//! 600,000 iterations (OWASP's 2023 floor for this construction). A
//! memory-hard KDF (Argon2id, scrypt) is strictly better against GPU
//! attackers and is the right next step; PBKDF2 is chosen here because it is
//! what the reference uses, so the two are comparable.
//!
//! **No key rotation, no recovery phrase, no second device.** Losing the
//! passphrase loses the identity. Said plainly rather than discovered.
//!
//! **The private key lives in memory once unlocked**, for the process's life.
//! Sealing it back per-use would need the passphrase per-use.

use std::fs;
use std::path::{Path, PathBuf};

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// ★ Named, not tuned. OWASP's 2023 floor for PBKDF2-HMAC-SHA256.
///
/// ★★ Kept as its own constant so the floor is still *asserted* even where the
/// cost is lowered — see `PBKDF2_ITERATIONS` below and the test that compares
/// them.
pub const PBKDF2_FLOOR: u32 = 600_000;

/// What this build actually pays to unlock an identity.
///
/// ★★★ **Lowered under `cfg(test)`, and that is a fix rather than a shortcut.**
/// The wire tests each unlock five identities and then talk over a socket with a
/// 20-second IO timeout. At the production cost a single unlock is around a
/// second alone and far longer under a parallel suite — so the peer's read times
/// out mid-handshake and four crypto tests fail. They had been recorded as a
/// flake; they were a **real interaction** between a production timeout and a
/// deliberately expensive KDF, and it fails more as the suite grows.
///
/// ★★★ Lowering the timeout's other side was the alternative and it is the
/// wrong one: 20 seconds is a real protection against a peer that stalls, and
/// weakening production to make a test suite comfortable is exactly backwards.
/// The wire tests are about the WIRE; the KDF's cost is asserted where it
/// belongs, against `PBKDF2_FLOOR`, in this module's own tests.
#[cfg(not(test))]
const PBKDF2_ITERATIONS: u32 = PBKDF2_FLOOR;
#[cfg(test)]
const PBKDF2_ITERATIONS: u32 = 1_000;

/// What the identity signs to prove it is itself. Constant on purpose: the
/// proof is about key possession, not about freshness.
const PROOF_MESSAGE: &[u8] = b"sustena:identity:v1";

/// The on-disk identity. ★ Everything here except `sealed_secret` is public.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityFile {
    pub handle: String,
    /// The verifying key, hex. Safe to publish — this is what a peer would get.
    pub public_key: String,
    pub kdf: String,
    pub salt: String,
    pub iterations: u32,
    /// The signing key, sealed under a key derived from the passphrase.
    pub sealed_secret: String,
    pub nonce: String,
    /// A signature over [`PROOF_MESSAGE`], so an unlock can be **verified**.
    pub proof: String,
}

/// An unlocked identity. ★★★ Holding one IS the authentication: it cannot be
/// constructed without a passphrase that genuinely decrypted the key.
#[derive(Clone)]
pub struct Unlocked {
    handle: String,
    signing: SigningKey,
}

impl std::fmt::Debug for Unlocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // ★ Never derive Debug on something holding a private key.
        f.debug_struct("Unlocked").field("handle", &self.handle).finish_non_exhaustive()
    }
}

impl Unlocked {
    /// The signing key's own bytes.
    ///
    /// ★★★ Crate-private, and it is the only way out. It exists for ONE
    /// caller -- `recovery_phrase`, which has to hand the key to a person so
    /// they can carry their identity to another device. Anything else wanting
    /// this wants to sign something, and should ask `sign` instead.
    pub(crate) fn secret_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    pub fn handle(&self) -> &str {
        &self.handle
    }

    pub fn public_key(&self) -> String {
        hex(self.signing.verifying_key().as_bytes())
    }

    /// Sign arbitrary bytes as this identity. The seam a peer transport uses.
    pub fn sign(&self, message: &[u8]) -> String {
        hex(&self.signing.sign(message).to_bytes())
    }
}

/// Check a signature against a claimed public key.
///
/// ★★★ The **peer** half of [`Unlocked::sign`], and a free function on
/// purpose: verifying is something this node does about SOMEONE ELSE, and
/// nothing about it should need an unlocked identity of its own. A locked node
/// can still tell a real signature from a forged one.
///
/// ★ Returns `bool` rather than a `Result` because every failure means the
/// same thing to the caller — *this is not who it says it is* — and there is
/// nothing an attacker should learn from which check failed. Malformed hex, a
/// key that is not on the curve, a wrong length and a genuinely bad signature
/// are all `false`.
pub fn verify(public_key: &str, message: &[u8], signature: &str) -> bool {
    let (Ok(key_bytes), Ok(sig_bytes)) = (unhex(public_key), unhex(signature)) else {
        return false;
    };
    let (Ok(key), Ok(sig)): (Result<[u8; 32], _>, Result<[u8; 64], _>) =
        (key_bytes.try_into(), sig_bytes.try_into())
    else {
        return false;
    };
    let Ok(verifying) = VerifyingKey::from_bytes(&key) else {
        return false;
    };
    verifying.verify(message, &Signature::from_bytes(&sig)).is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    NotEnrolled,
    AlreadyEnrolled,
    /// ★ One message for a wrong passphrase AND a tampered file. Distinguishing
    /// them would tell an attacker which half they got right.
    Unlock,
    WeakPassphrase(String),
    Io(String),
    Malformed(String),
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotEnrolled => write!(f, "no identity on this machine yet"),
            Self::AlreadyEnrolled => write!(f, "an identity already exists here"),
            Self::Unlock => write!(f, "that passphrase does not unlock this identity"),
            Self::WeakPassphrase(w) => write!(f, "{w}"),
            Self::Io(e) => write!(f, "the identity file could not be read or written: {e}"),
            Self::Malformed(e) => write!(f, "the identity file is not readable: {e}"),
        }
    }
}

impl std::error::Error for IdentityError {}

/// The identity store: one file, beside the event logs.
pub struct IdentityStore {
    path: PathBuf,
}

impl IdentityStore {
    pub fn at(root: impl AsRef<Path>) -> Self {
        Self { path: root.as_ref().join("identity.json") }
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn read(&self) -> Result<IdentityFile, IdentityError> {
        if !self.exists() {
            return Err(IdentityError::NotEnrolled);
        }
        let text = fs::read_to_string(&self.path).map_err(|e| IdentityError::Io(e.to_string()))?;
        serde_json::from_str(&text).map_err(|e| IdentityError::Malformed(e.to_string()))
    }

    /// The public half, for a screen that wants to show who this machine is
    /// **without** unlocking anything.
    pub fn handle(&self) -> Option<String> {
        self.read().ok().map(|f| f.handle)
    }

    /// **Enrol**: mint a keypair, seal it under the passphrase, write the file.
    ///
    /// ★ Refuses to overwrite. An enrol that silently replaced an existing
    /// identity would destroy the only copy of a private key.
    pub fn enrol(&self, handle: &str, passphrase: &str) -> Result<Unlocked, IdentityError> {
        if self.exists() {
            return Err(IdentityError::AlreadyEnrolled);
        }
        check_passphrase(passphrase)?;

        let mut secret = [0u8; 32];
        fill_random(&mut secret)?;
        let signing = SigningKey::from_bytes(&secret);

        let mut salt = [0u8; 16];
        fill_random(&mut salt)?;
        let mut nonce_bytes = [0u8; 24];
        fill_random(&mut nonce_bytes)?;

        let key = derive(passphrase, &salt, PBKDF2_ITERATIONS);
        let cipher = XChaCha20Poly1305::new((&key).into());
        let sealed = cipher
            .encrypt(XNonce::from_slice(&nonce_bytes), signing.to_bytes().as_slice())
            .map_err(|_| IdentityError::Io("sealing the key failed".into()))?;

        let file = IdentityFile {
            handle: handle.to_string(),
            public_key: hex(signing.verifying_key().as_bytes()),
            kdf: "pbkdf2-hmac-sha256".to_string(),
            salt: hex(&salt),
            iterations: PBKDF2_ITERATIONS,
            sealed_secret: hex(&sealed),
            nonce: hex(&nonce_bytes),
            proof: hex(&signing.sign(PROOF_MESSAGE).to_bytes()),
        };

        // Temp-then-rename, like the registry: a crash mid-write must not
        // leave a truncated identity, because there is no second copy.
        let text =
            serde_json::to_string_pretty(&file).map_err(|e| IdentityError::Io(e.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, text).map_err(|e| IdentityError::Io(e.to_string()))?;
        fs::rename(&tmp, &self.path).map_err(|e| IdentityError::Io(e.to_string()))?;

        Ok(Unlocked { handle: file.handle, signing })
    }

    /// Where a cached unlock key lives, when one is allowed to exist.
    fn cache_path(&self) -> PathBuf {
        self.path.with_file_name("local_unlock.key")
    }

    /// Whether this node can come up without being asked for a passphrase.
    pub fn has_cached_unlock(&self) -> bool {
        self.cache_path().exists()
    }

    /// **Remember the unlock**, so this node never stalls waiting for a person.
    ///
    /// ★★★ **What this genuinely costs, stated plainly.** The cached value
    /// unseals the private key. Anyone who can read the file can act as this
    /// node. It is off by default, it is never written unless somebody asks
    /// for it in so many words, and deleting the file restores the passphrase
    /// gate exactly as it was -- the sealed identity is untouched.
    ///
    /// ★★ The *derived* key is cached rather than the passphrase. It unseals
    /// this identity file and nothing else, so a passphrase reused elsewhere is
    /// not put at risk by a machine-local convenience.
    ///
    /// ★ Written only after the passphrase has actually opened the identity, so
    /// a wrong one can never be cached.
    pub fn remember_unlock(&self, passphrase: &str) -> Result<(), IdentityError> {
        self.unlock(passphrase)?;
        let file = self.read()?;
        let salt = unhex(&file.salt)?;
        let key = derive(passphrase, &salt, file.iterations);
        let tmp = self.cache_path().with_extension("tmp");
        fs::write(&tmp, hex(&key)).map_err(|e| IdentityError::Io(e.to_string()))?;
        fs::rename(&tmp, self.cache_path()).map_err(|e| IdentityError::Io(e.to_string()))
    }

    /// Stop remembering. ★ The sealed identity is untouched, so this is a
    /// complete undo and not a repair.
    pub fn forget_unlock(&self) -> Result<(), IdentityError> {
        match fs::remove_file(self.cache_path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(IdentityError::Io(e.to_string())),
        }
    }

    /// Unlock from the remembered key, if there is one.
    ///
    /// ★ Every check `unlock` makes still runs -- the cached key only replaces
    /// the derivation, not the proof that the recovered key is this identity.
    pub fn unlock_remembered(&self) -> Result<Unlocked, IdentityError> {
        let hexed = fs::read_to_string(self.cache_path()).map_err(|_| IdentityError::Unlock)?;
        let key = unhex(hexed.trim())?;
        let key: [u8; 32] = key.try_into().map_err(|_| IdentityError::Unlock)?;
        self.open_with(&key)
    }

    /// **Unlock**: recover the private key, then make it prove itself.
    ///
    /// ★★★ There is no comparison against a stored answer anywhere in here.
    /// A wrong passphrase derives a wrong key and the AEAD tag fails — the
    /// failure is cryptographic, not a branch somebody could invert.
    pub fn unlock(&self, passphrase: &str) -> Result<Unlocked, IdentityError> {
        let file = self.read()?;
        if file.kdf != "pbkdf2-hmac-sha256" {
            return Err(IdentityError::Malformed(format!("unknown kdf '{}'", file.kdf)));
        }
        let salt = unhex(&file.salt)?;
        let nonce = unhex(&file.nonce)?;
        let sealed = unhex(&file.sealed_secret)?;
        if nonce.len() != 24 {
            return Err(IdentityError::Malformed("the nonce is the wrong length".into()));
        }

        let key = derive(passphrase, &salt, file.iterations);
        let cipher = XChaCha20Poly1305::new((&key).into());
        let opened = cipher
            .decrypt(XNonce::from_slice(&nonce), sealed.as_slice())
            .map_err(|_| IdentityError::Unlock)?;
        let bytes: [u8; 32] = opened.try_into().map_err(|_| IdentityError::Unlock)?;
        let signing = SigningKey::from_bytes(&bytes);

        // ★★ The recovered key must match the PUBLIC half on file, and must
        //    reproduce the stored proof. Decryption succeeding is already
        //    strong; this makes the identity's claim about itself testable by
        //    anything holding only the public file.
        let declared = unhex(&file.public_key)?;
        let declared: [u8; 32] =
            declared.try_into().map_err(|_| IdentityError::Malformed("public key".into()))?;
        let verifying = VerifyingKey::from_bytes(&declared)
            .map_err(|_| IdentityError::Malformed("public key is not on the curve".into()))?;
        if signing.verifying_key() != verifying {
            return Err(IdentityError::Unlock);
        }
        let proof = unhex(&file.proof)?;
        let proof: [u8; 64] =
            proof.try_into().map_err(|_| IdentityError::Malformed("proof".into()))?;
        verifying
            .verify(PROOF_MESSAGE, &Signature::from_bytes(&proof))
            .map_err(|_| IdentityError::Unlock)?;

        Ok(Unlocked { handle: file.handle, signing })
    }

    /// Open the identity with an already-derived key.
    ///
    /// ★★ Deliberately runs the SAME checks `unlock` does after decryption --
    /// the recovered key must match the public half and reproduce the stored
    /// proof. A cached key is a shortcut past the KDF, never past the proof.
    fn open_with(&self, key: &[u8; 32]) -> Result<Unlocked, IdentityError> {
        let file = self.read()?;
        let nonce = unhex(&file.nonce)?;
        let sealed = unhex(&file.sealed_secret)?;
        let cipher = XChaCha20Poly1305::new(key.into());
        let opened = cipher
            .decrypt(XNonce::from_slice(&nonce), sealed.as_slice())
            .map_err(|_| IdentityError::Unlock)?;
        let bytes: [u8; 32] = opened.try_into().map_err(|_| IdentityError::Unlock)?;
        let signing = SigningKey::from_bytes(&bytes);
        let declared = unhex(&file.public_key)?;
        let declared: [u8; 32] =
            declared.try_into().map_err(|_| IdentityError::Malformed("public key".into()))?;
        let verifying = VerifyingKey::from_bytes(&declared)
            .map_err(|_| IdentityError::Malformed("public key is not on the curve".into()))?;
        if signing.verifying_key() != verifying {
            return Err(IdentityError::Unlock);
        }
        let proof = unhex(&file.proof)?;
        let proof: [u8; 64] =
            proof.try_into().map_err(|_| IdentityError::Malformed("proof".into()))?;
        verifying
            .verify(PROOF_MESSAGE, &Signature::from_bytes(&proof))
            .map_err(|_| IdentityError::Unlock)?;
        Ok(Unlocked { handle: file.handle, signing })
    }
}

/// ★ A floor, stated. Not a strength meter: a rule a person can read.
fn check_passphrase(p: &str) -> Result<(), IdentityError> {
    if p.chars().count() < 12 {
        return Err(IdentityError::WeakPassphrase(
            "a passphrase needs at least 12 characters — this is the only thing between \
             a stolen laptop and the private key"
                .into(),
        ));
    }
    Ok(())
}

fn derive(passphrase: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, iterations, &mut key);
    key
}

fn fill_random(buf: &mut [u8]) -> Result<(), IdentityError> {
    getrandom::getrandom(buf).map_err(|e| IdentityError::Io(format!("no entropy: {e}")))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>, IdentityError> {
    if s.len() % 2 != 0 {
        return Err(IdentityError::Malformed("odd-length hex".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| IdentityError::Malformed(e.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-identity-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    const PASS: &str = "a real passphrase";

    #[test]
    fn enrol_then_unlock_recovers_the_same_identity() {
        let s = IdentityStore::at(scratch("roundtrip"));
        let a = s.enrol("bg.myc", PASS).expect("enrol");
        let b = s.unlock(PASS).expect("unlock");
        assert_eq!(a.handle(), "bg.myc");
        assert_eq!(a.public_key(), b.public_key());
        // The same key produces the same signature over the same message.
        assert_eq!(a.sign(b"hello"), b.sign(b"hello"));
    }

    #[test]
    fn a_wrong_passphrase_fails_closed() {
        let s = IdentityStore::at(scratch("wrong"));
        s.enrol("bg.myc", PASS).expect("enrol");
        for attempt in ["a real passphras", "A REAL PASSPHRASE", "", "wrong entirely!!"] {
            assert_eq!(
                s.unlock(attempt).unwrap_err(),
                IdentityError::Unlock,
                "{attempt:?} must not unlock"
            );
        }
    }

    #[test]
    fn a_tampered_sealed_key_fails_closed_with_the_same_message() {
        let dir = scratch("tamper");
        let s = IdentityStore::at(&dir);
        s.enrol("bg.myc", PASS).expect("enrol");
        let mut f = s.read().expect("read");
        // Flip one hex digit of the sealed key.
        let mut chars: Vec<char> = f.sealed_secret.chars().collect();
        chars[0] = if chars[0] == 'a' { 'b' } else { 'a' };
        f.sealed_secret = chars.into_iter().collect();
        fs::write(dir.join("identity.json"), serde_json::to_string(&f).unwrap()).unwrap();
        // ★ Same error as a wrong passphrase: telling them apart would tell an
        //   attacker which half they got right.
        assert_eq!(s.unlock(PASS).unwrap_err(), IdentityError::Unlock);
    }

    #[test]
    fn a_swapped_public_key_is_caught_even_though_decryption_succeeded() {
        let dir = scratch("swap");
        let s = IdentityStore::at(&dir);
        s.enrol("bg.myc", PASS).expect("enrol");
        let mut f = s.read().expect("read");
        // A different, VALID public key. Decryption still works; the identity
        // no longer matches what the file claims about itself.
        let other = IdentityStore::at(scratch("swap-other"));
        f.public_key = other.enrol("other", PASS).expect("enrol").public_key();
        fs::write(dir.join("identity.json"), serde_json::to_string(&f).unwrap()).unwrap();
        assert_eq!(s.unlock(PASS).unwrap_err(), IdentityError::Unlock);
    }

    #[test]
    fn enrolling_twice_refuses_rather_than_overwriting_a_private_key() {
        let s = IdentityStore::at(scratch("twice"));
        s.enrol("bg.myc", PASS).expect("enrol");
        assert_eq!(s.enrol("bg.myc", PASS).unwrap_err(), IdentityError::AlreadyEnrolled);
        // And the original still unlocks.
        assert!(s.unlock(PASS).is_ok());
    }

    #[test]
    fn a_short_passphrase_is_refused_at_enrol() {
        let s = IdentityStore::at(scratch("short"));
        assert!(matches!(s.enrol("bg.myc", "short").unwrap_err(), IdentityError::WeakPassphrase(_)));
        assert!(!s.exists(), "nothing is written when the passphrase is refused");
    }

    #[test]
    fn unlocking_before_enrolling_says_so() {
        let s = IdentityStore::at(scratch("empty"));
        assert_eq!(s.unlock(PASS).unwrap_err(), IdentityError::NotEnrolled);
    }

    #[test]
    fn two_identities_are_genuinely_different_keys() {
        let a = IdentityStore::at(scratch("k1")).enrol("a", PASS).expect("enrol");
        let b = IdentityStore::at(scratch("k2")).enrol("b", PASS).expect("enrol");
        assert_ne!(a.public_key(), b.public_key(), "keygen must not be deterministic");
    }

    #[test]
    fn the_private_key_never_appears_in_debug_output() {
        let s = IdentityStore::at(scratch("debug"));
        let u = s.enrol("bg.myc", PASS).expect("enrol");
        let rendered = format!("{u:?}");
        assert!(rendered.contains("bg.myc"));
        assert!(!rendered.contains(&u.sign(b"x")), "no key material in Debug");
    }
}

#[cfg(test)]
mod kdf_cost_tests {
    use super::*;

    #[test]
    fn the_production_cost_is_still_owasps_floor() {
        // ★★★ The point of keeping the floor as its own constant: the test
        //     build pays less so the suite can run, and the number that ships
        //     is still guarded here. A single constant lowered for tests would
        //     have quietly shipped a weaker one.
        assert_eq!(PBKDF2_FLOOR, 600_000);
    }

    #[test]
    fn the_test_build_pays_less_and_says_so() {
        // ★★ Named rather than left for somebody to discover while wondering
        //    why a local unlock feels instant.
        const {
            assert!(
                PBKDF2_ITERATIONS < PBKDF2_FLOOR,
                "tests run at a reduced cost on purpose — see the constant's note"
            )
        };
    }

    #[test]
    fn an_identity_still_unlocks_with_the_cost_it_was_written_at() {
        // ★★★ The iteration count travels with the FILE rather than being
        //     assumed from the build, so an identity written at one cost still
        //     opens after the constant changes.
        let dir = std::env::temp_dir().join("mycelium-kdf-cost");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        let store = IdentityStore::at(&dir);
        store.enrol("bonnie", "a-long-enough-passphrase").expect("enrolled");
        assert!(store.unlock("a-long-enough-passphrase").is_ok());
    }
}

// ── carrying an identity to another device ──────────────────────────────────
//
// ★★★ **The problem this solves.** `identity.json` is a local file. A second
// device has none, so a passphrase there means nothing and enrolling mints a
// NEW keypair — a different principal, which cannot claim his Sustains and is
// not him. "Log in on my laptop and my data is there" is impossible until an
// identity can travel.
//
// ★★★ **Why the phrase IS the key, rather than something the key is derived
// from.** Deriving a key from passphrase-plus-phrase is the tidier story, and
// it cannot rescue the identity he ALREADY has: that one was generated from
// random bytes months ago and no derivation will ever reproduce it. An
// identity that cannot be carried is exactly the problem. So the phrase
// encodes the secret itself, which works for every identity that exists today
// and every one made after — the same model a wallet seed phrase uses.
//
// ★★ **What that costs, stated plainly.** The phrase is the key. Anyone
// holding it is him. It is shown once, it is never stored anywhere by this
// app, and it is worth writing on paper rather than keeping in a photo.
//
// ★ **Crockford base32, not words.** A word list means embedding and
// maintaining two thousand words and getting a checksum scheme right; getting
// it subtly wrong produces phrases that fail to restore, which is the worst
// possible failure for a recovery mechanism. Crockford's alphabet already
// excludes the characters people confuse (I, L, O, U) and defines how to fold
// the ones they still mistype, so a transcription slip is corrected rather
// than rejected. A friendlier word list is a later change that can reuse all
// of this by swapping the encoding.

/// Crockford's alphabet: no I, L, O or U, so there is nothing to confuse with
/// 1, 0 or each other.
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The check appended to a phrase, so a mistyped one is refused rather than
/// silently restoring a different -- and wrong -- identity.
///
/// ★★★ **TWO symbols, from a position-sensitive sum.** The first version was
/// one symbol from a plain byte sum, which is 1-in-32 to accept a typo by
/// coincidence and blind to transposition entirely -- a sum does not care what
/// order the bytes came in. A test caught it by getting unlucky, which is
/// exactly how a person would have found it: a phrase that restored the wrong
/// identity in silence.
///
/// ★★ Ten bits and order-sensitive is enough here. This is not defending
/// against an adversary -- somebody who can choose the phrase already has the
/// key -- it is defending against a hand copying twenty-six characters off
/// paper, where the realistic mistakes are one wrong symbol and two swapped.
fn check_symbols(bytes: &[u8]) -> [char; 2] {
    let (mut a, mut b) = (1u32, 0u32);
    for byte in bytes {
        a = (a + u32::from(*byte)) % 1021;
        b = (b + a) % 1021;
    }
    let sum = (b << 10) | a;
    [CROCKFORD[((sum >> 5) & 31) as usize] as char, CROCKFORD[(sum & 31) as usize] as char]
}

fn crockford_value(c: char) -> Option<u8> {
    let c = c.to_ascii_uppercase();
    // ★★ The letters people actually mistype, folded to what they meant.
    let c = match c {
        'I' | 'L' => '1',
        'O' => '0',
        'U' => 'V',
        other => other,
    };
    CROCKFORD.iter().position(|b| *b as char == c).map(|i| i as u8)
}

/// The 32 secret bytes, as something a person can write down.
pub fn phrase_of(secret: &[u8; 32]) -> String {
    let mut bits = 0u32;
    let mut nbits = 0u32;
    let mut out = String::new();
    for b in secret {
        bits = (bits << 8) | u32::from(*b);
        nbits += 8;
        while nbits >= 5 {
            nbits -= 5;
            out.push(CROCKFORD[((bits >> nbits) & 31) as usize] as char);
        }
    }
    if nbits > 0 {
        out.push(CROCKFORD[((bits << (5 - nbits)) & 31) as usize] as char);
    }
    for c in check_symbols(secret) {
        out.push(c);
    }
    // ★ Grouped, because a wall of characters is where transcription goes
    //   wrong. The groups are cosmetic and stripped on the way back in.
    out.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect::<Vec<_>>()
        .join("-")
}

/// The secret those words stand for.
///
/// ★★ Whitespace, dashes and case are all forgiven: a person copying this off
/// paper should not be defeated by how they spaced it.
pub fn secret_of(phrase: &str) -> Result<[u8; 32], IdentityError> {
    let cleaned: Vec<char> = phrase
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .collect();
    if cleaned.len() < 3 {
        return Err(IdentityError::Malformed("that recovery phrase is too short".into()));
    }
    let (body, check) = cleaned.split_at(cleaned.len() - 2);
    let mut bits = 0u32;
    let mut nbits = 0u32;
    let mut bytes: Vec<u8> = Vec::with_capacity(32);
    for c in body {
        let v = crockford_value(*c).ok_or_else(|| {
            IdentityError::Malformed(format!("'{c}' is not part of a recovery phrase"))
        })?;
        bits = (bits << 5) | u32::from(v);
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            bytes.push(((bits >> nbits) & 0xff) as u8);
        }
    }
    if bytes.len() != 32 {
        return Err(IdentityError::Malformed(format!(
            "a recovery phrase carries 32 bytes; this one carries {}",
            bytes.len()
        )));
    }
    let mut secret = [0u8; 32];
    secret.copy_from_slice(&bytes);
    // ★★★ The check runs BEFORE anything is written. A mistyped phrase is a
    //     DIFFERENT valid-looking key, so without this a typo would silently
    //     enrol a stranger's identity on his laptop and nothing would say so
    //     until his data failed to arrive.
    // ★★ Folded the same way the body is. The check symbol is drawn from the
    //    same alphabet, so a person who correctly wrote an O for a 0 in THAT
    //    position was having a correct transcription refused -- which a test
    //    caught, and which would have read as "your phrase is wrong" to
    //    somebody who had copied it perfectly.
    let mut given = ['0'; 2];
    for (i, c) in check.iter().enumerate() {
        given[i] = crockford_value(*c)
            .map(|v| CROCKFORD[v as usize] as char)
            .ok_or_else(|| IdentityError::Malformed("that recovery phrase ends oddly".into()))?;
    }
    if check_symbols(&secret) != given {
        return Err(IdentityError::Malformed(
            "that recovery phrase has a typo in it somewhere".into(),
        ));
    }
    Ok(secret)
}

impl IdentityStore {
    /// **The phrase that carries this identity to another device.**
    ///
    /// ★★ Requires the passphrase, because it hands back the key itself. It is
    /// never stored: it is derived from the sealed secret on demand, so there
    /// is no second copy of his key anywhere on disk.
    pub fn recovery_phrase(&self, passphrase: &str) -> Result<String, IdentityError> {
        let unlocked = self.unlock(passphrase)?;
        Ok(phrase_of(&unlocked.secret_bytes()))
    }

    /// **Become the same person on a device that has never seen him.**
    ///
    /// ★★★ This is what makes a second device HIM rather than a new principal.
    /// The keypair is not generated, it is restored, so the public key — which
    /// is what owns his Sustains and what a peer authenticates — is identical
    /// to the one on his phone.
    ///
    /// ★★ The passphrase given here seals the key on THIS device and need not
    /// match the other one. A passphrase is how a machine keeps a secret; the
    /// phrase is who he is.
    pub fn restore(
        &self,
        handle: &str,
        passphrase: &str,
        phrase: &str,
    ) -> Result<Unlocked, IdentityError> {
        if self.exists() {
            return Err(IdentityError::AlreadyEnrolled);
        }
        check_passphrase(passphrase)?;
        let secret = secret_of(phrase)?;
        let signing = SigningKey::from_bytes(&secret);

        let mut salt = [0u8; 16];
        fill_random(&mut salt)?;
        let mut nonce_bytes = [0u8; 24];
        fill_random(&mut nonce_bytes)?;
        let key = derive(passphrase, &salt, PBKDF2_ITERATIONS);
        let cipher = XChaCha20Poly1305::new((&key).into());
        let sealed = cipher
            .encrypt(XNonce::from_slice(&nonce_bytes), signing.to_bytes().as_slice())
            .map_err(|_| IdentityError::Io("sealing the key failed".into()))?;

        let file = IdentityFile {
            handle: handle.to_string(),
            public_key: hex(signing.verifying_key().as_bytes()),
            kdf: "pbkdf2-hmac-sha256".to_string(),
            salt: hex(&salt),
            iterations: PBKDF2_ITERATIONS,
            sealed_secret: hex(&sealed),
            nonce: hex(&nonce_bytes),
            proof: hex(&signing.sign(PROOF_MESSAGE).to_bytes()),
        };
        let text =
            serde_json::to_string_pretty(&file).map_err(|e| IdentityError::Io(e.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, text).map_err(|e| IdentityError::Io(e.to_string()))?;
        fs::rename(&tmp, &self.path).map_err(|e| IdentityError::Io(e.to_string()))?;
        self.unlock(passphrase)
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use std::{env, fs, path::PathBuf};

    fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("sustena-recovery-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    const PASS: &str = "a good long passphrase";

    #[test]
    fn a_phrase_carries_the_key_there_and_back() {
        let secret = [7u8; 32];
        assert_eq!(secret_of(&phrase_of(&secret)).expect("round trip"), secret);
    }

    #[test]
    fn the_same_person_appears_on_a_second_device() {
        // ★★★ The whole point. A second device must be HIM -- the same public
        //     key, which is what owns his Sustains and what a peer
        //     authenticates -- not a new principal that can claim nothing.
        let phone = IdentityStore::at(scratch("phone"));
        let him = phone.enrol("bg.myc", PASS).expect("enrolled");
        let phrase = phone.recovery_phrase(PASS).expect("phrase");

        let laptop = IdentityStore::at(scratch("laptop"));
        // ★★ A DIFFERENT passphrase on the new machine, deliberately. A
        //    passphrase is how one machine keeps a secret; the phrase is who
        //    he is.
        let same = laptop.restore("bg.myc", "another good passphrase", &phrase).expect("restored");
        assert_eq!(same.public_key(), him.public_key());
    }

    #[test]
    fn a_typo_is_refused_rather_than_restoring_a_stranger() {
        // ★★★ The failure that must not happen quietly. A mistyped phrase is a
        //     DIFFERENT valid-looking key, so without a check it would enrol
        //     somebody else's identity on his laptop and nothing would say so
        //     until his data failed to arrive.
        let phone = IdentityStore::at(scratch("typo"));
        phone.enrol("bg.myc", PASS).expect("enrolled");
        let phrase = phone.recovery_phrase(PASS).expect("phrase");

        let mut wrong: Vec<char> = phrase.chars().collect();
        let i = wrong.iter().position(|c| c.is_ascii_alphanumeric()).expect("a symbol");
        wrong[i] = if wrong[i] == '7' { '9' } else { '7' };
        let wrong: String = wrong.into_iter().collect();

        let laptop = IdentityStore::at(scratch("typo-laptop"));
        let err = laptop.restore("bg.myc", PASS, &wrong).unwrap_err();
        assert!(matches!(err, IdentityError::Malformed(_)), "got {err:?}");
        assert!(!laptop.exists(), "and nothing was written");
    }

    #[test]
    fn every_single_symbol_typo_is_caught() {
        // ★★★ Exhaustive, not sampled, because the previous check passed for a
        //     while by luck: a one-symbol sum accepts a wrong phrase once in
        //     thirty-two, and the test that found it did so by drawing an
        //     unlucky random key. A recovery phrase that silently restores the
        //     WRONG identity is the worst failure this code has, so it is
        //     proven over every position and every substitution rather than
        //     over one random attempt.
        let secret = [0x5au8; 32];
        let phrase = phrase_of(&secret);
        let symbols: Vec<char> = phrase.chars().filter(|c| *c != '-').collect();
        let mut checked = 0;
        for i in 0..symbols.len() {
            for &c in CROCKFORD.iter() {
                let c = c as char;
                if c == symbols[i] {
                    continue;
                }
                let mut wrong = symbols.clone();
                wrong[i] = c;
                let wrong: String = wrong.into_iter().collect();
                // ★★ The danger is a typo that decodes to a DIFFERENT key
                //    without being refused -- that restores a stranger's
                //    identity in silence. Being refused is safe, and so is
                //    decoding to the same key: the final symbol carries
                //    padding bits that are discarded, so altering it changes
                //    nothing about the secret. Both are fine; a silent
                //    substitution is not.
                match secret_of(&wrong) {
                    Err(_) => {}
                    Ok(other) => assert_eq!(
                        other, secret,
                        "a one-symbol typo silently restored a DIFFERENT key: position {i} -> {c}"
                    ),
                }
                checked += 1;
            }
        }
        assert!(checked > 1000, "it really did try them all: {checked}");
    }

    #[test]
    fn two_swapped_symbols_are_caught() {
        // ★★ The other mistake a hand makes. A plain sum is blind to it --
        //    it does not care what order the bytes arrived in.
        let secret = [0x11u8; 32];
        let phrase = phrase_of(&secret);
        let mut symbols: Vec<char> = phrase.chars().filter(|c| *c != '-').collect();
        let mut swapped = 0;
        for i in 0..symbols.len() - 1 {
            if symbols[i] == symbols[i + 1] {
                continue;
            }
            symbols.swap(i, i + 1);
            let s: String = symbols.iter().collect();
            match secret_of(&s) {
                Err(_) => {}
                Ok(other) => assert_eq!(
                    other, secret,
                    "a transposition at {i} silently restored a DIFFERENT key"
                ),
            }
            symbols.swap(i, i + 1);
            swapped += 1;
        }
        assert!(swapped > 0, "there was something to swap");
    }

    #[test]
    fn how_he_spaced_it_does_not_matter() {
        // ★★ Copied off paper. Spacing, case and dashes are his business.
        let secret = [42u8; 32];
        let phrase = phrase_of(&secret);
        let messy = phrase.to_lowercase().replace('-', "  ");
        assert_eq!(secret_of(&messy).expect("forgiving"), secret);
    }

    #[test]
    fn the_letters_people_confuse_are_folded_not_rejected() {
        // ★★ Crockford's whole point: O is 0 and I is 1, because on paper they
        //    are the same shape. Rejecting them would fail a correct
        //    transcription of a correct phrase.
        let secret = [0u8; 32];
        let phrase = phrase_of(&secret);
        let confused = phrase.replace('0', "O");
        assert_eq!(secret_of(&confused).expect("folded"), secret);
    }

    #[test]
    fn nonsense_is_refused_with_something_readable() {
        let laptop = IdentityStore::at(scratch("nonsense"));
        for bad in ["", "hello", "not a phrase at all"] {
            let err = laptop.restore("bg.myc", PASS, bad).unwrap_err();
            assert!(matches!(err, IdentityError::Malformed(_)), "{bad:?} -> {err:?}");
        }
    }

    #[test]
    fn a_device_that_already_has_an_identity_refuses_to_be_overwritten() {
        // ★★★ Restoring over a live identity would orphan every Sustain that
        //     identity owns. It is refused, and the message says so.
        let store = IdentityStore::at(scratch("occupied"));
        store.enrol("bg.myc", PASS).expect("enrolled");
        let phrase = store.recovery_phrase(PASS).expect("phrase");
        assert!(matches!(
            store.restore("bg.myc", PASS, &phrase).unwrap_err(),
            IdentityError::AlreadyEnrolled
        ));
    }

    #[test]
    fn the_phrase_needs_the_passphrase() {
        // ★ It hands back the key itself, so it is not something a passer-by
        //   can read off an unlocked screen.
        let store = IdentityStore::at(scratch("guarded"));
        store.enrol("bg.myc", PASS).expect("enrolled");
        assert!(store.recovery_phrase("the wrong passphrase").is_err());
    }

    #[test]
    fn an_identity_made_before_any_of_this_can_still_travel() {
        // ★★★ The reason the phrase encodes the key rather than deriving from
        //     a passphrase. His identity was generated from random bytes long
        //     before recovery existed; a derivation could never reproduce it,
        //     and an identity that cannot be carried is the whole problem.
        let old = IdentityStore::at(scratch("legacy"));
        let him = old.enrol("bg.myc", PASS).expect("enrolled the old way");
        let carried = old.recovery_phrase(PASS).expect("it still exports");
        let new = IdentityStore::at(scratch("legacy-new"));
        assert_eq!(
            new.restore("bg.myc", PASS, &carried).expect("restored").public_key(),
            him.public_key()
        );
    }
}
