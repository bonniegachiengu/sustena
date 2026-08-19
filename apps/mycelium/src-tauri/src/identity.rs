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
const PBKDF2_ITERATIONS: u32 = 600_000;

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
