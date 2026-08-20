//! **The arena** — the package registry, and the two things a registry owes.
//!
//! Storage is the host's, like every other store: an append-only
//! `packages.jsonl` beside the logs, and an `installs.jsonl` recording what
//! this node actually took. The **judgment** is `sustena_core::package`'s, and
//! this file cannot get around it — an install needs an `Admitted`, and only
//! the core's gates produce one.
//!
//! ★★★ **A registry owes two things and they are different.** *Are these the
//! bytes that were published* (a content hash) and *did that key publish them*
//! (an ed25519 signature over that hash) are computed here because sha2 and
//! ed25519 live in the host. Whether the thing is any GOOD is neither — that
//! is provenance, and it is shown rather than enforced. A package from a key
//! you would trust with anything still faces `editing::typecheck`.
//!
//! ★★ **The hash is over a canonical form, not over the file.** Two nodes that
//! serialise the same artifact with different key order must agree it is the
//! same artifact, or every peer transfer would read as tampering. `canonical`
//! sorts object keys recursively; the hash is over that.
//!
//! ★★★ **Publishing runs the gate too.** A package that does not typecheck is
//! **refused at publish**, not stored and refused later. Storing a known-broken
//! artifact would make the registry a place where bad things wait — and the
//! reference does exactly that (`spec_json` goes in unvalidated, and nothing
//! ever reads it back).
//!
//! **v1 scope, named:** a **local** registry — publish, browse, install on this
//! node. Packages can travel over slice 6's transport, and that increment is
//! not built here: it needs a `Frame` variant and a decision about whether a
//! peer's catalogue is pulled or pushed, which is a design question rather than
//! a missing line.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use sustena_core::package::{
    Authenticity, Integrity, Kind, Origin, PackageProvenance,
};

use crate::identity::verify;

/// What a signature covers. ★ A domain tag, so a signature made over a package
/// hash can never be replayed as a peer handshake proof or anything else this
/// key signs.
const SIGN_DOMAIN: &str = "SUSTENA-PACKAGE-1";

// ---------------------------------------------------------------------------
// The record
// ---------------------------------------------------------------------------

/// One published package, as stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// The artifact itself, in the host's own authored form.
    pub spec: Value,
    /// The author's ed25519 public key, hex. **The identity.**
    pub author: String,
    /// A label the author chose. Advisory.
    #[serde(default)]
    pub author_handle: String,
    /// sha256 over [`canonical`] of `spec`, hex.
    pub content_hash: String,
    /// Over `SUSTENA-PACKAGE-1|<content_hash>`. ★ `None` is **unsigned**, which
    /// is not the same as a bad signature — see `Authenticity`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Where this node got it.
    pub origin: Origin,
    /// What it charges to use, in **juul** — the internal unit. Never money.
    #[serde(default)]
    pub per_mille: u32,
    pub published_at: u64,
}

impl Package {
    /// The three questions, answered — integrity and authenticity recomputed
    /// from the bytes on disk rather than trusted from the record.
    ///
    /// ★★★ Recomputed **every time**, not cached at publish. A hash stored
    /// beside the thing it hashes and never re-checked proves nothing; the
    /// whole point is to catch a `spec` that changed after the fact.
    pub fn provenance(&self) -> PackageProvenance {
        let actual = content_hash(&self.spec);
        let integrity = if actual == self.content_hash {
            Integrity::Intact
        } else {
            Integrity::Altered { claimed: self.content_hash.clone(), actual }
        };
        let authenticity = match &self.signature {
            None => Authenticity::Unsigned,
            Some(sig) => {
                if verify(&self.author, signed_message(&self.content_hash).as_bytes(), sig) {
                    Authenticity::Signed
                } else {
                    Authenticity::Forged
                }
            }
        };
        PackageProvenance {
            author: self.author.clone(),
            origin: self.origin.clone(),
            integrity,
            authenticity,
        }
    }
}

/// What a person is asking to publish, before it is stamped or judged.
///
/// ★ One struct rather than nine parameters: the metadata travels together
/// everywhere it goes — the form, the command, the store — so it may as well
/// be one thing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Publication {
    pub name: String,
    /// ★ A string at the boundary, parsed by `Kind::parse` — `Kind` is a core
    /// type and specta is a host dependency, so the enum cannot cross as
    /// itself. An unknown kind is refused rather than defaulted.
    pub kind: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub spec: Value,
    /// The royalty rate in **juul** per mille. `0` is [`Licence::Free`].
    #[serde(default)]
    pub per_mille: u32,
    /// The Sustain a widget would join. Required for a widget, meaningless
    /// for a definition — see `World::judge`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub into: Option<String>,
}

/// What this node installed, and from what.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Install {
    pub package_id: String,
    pub kind: Kind,
    /// The artifact's own id once installed — a definition id, a widget id.
    pub artifact_id: String,
    /// For a widget: which Sustain it was installed into. A definition is
    /// node-wide, so `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub into: Option<String>,
    /// The hash at install time. ★ Kept so a later tamper is visible as a
    /// difference from what was actually accepted, not just from what the
    /// record claims.
    pub content_hash: String,
    pub installed_at: u64,
}

/// An acquisition: who took which package, when, and what it cost in **juul**.
///
/// ★★★ **Parity on the record shape, and deliberately NOT on half of it.**
/// The reference's `arena_orders` carries `delivery_addr`, `pay_method` and
/// `product_total` — real-world commerce for physical goods sold through the
/// same table. Importing those fields would put a payment method one struct
/// away from a package install, and ADR-0001 D5 exists precisely to keep that
/// distance infinite. What is kept is the half that describes an acquisition:
/// a reference, who, what, when, and the licence terms that applied.
///
/// ★★ `paid` is **juul** — this host's internal accounting unit. Never money,
/// never transferable off this host, never a rail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    /// A short human reference, the reference's own `SXI-` shape.
    pub reference: String,
    pub package_id: String,
    pub package_name: String,
    /// The acquiring principal's handle.
    pub by: String,
    /// Juul actually transferred. `0` for a free package — free of royalty,
    /// never free of the work it causes.
    pub paid: u64,
    /// `(role, recipient, amount)` for every share, including ones that stayed
    /// where they were.
    #[serde(default)]
    pub shares: Vec<(String, String, u64)>,
    /// The licence rate that applied, in juul per mille.
    pub per_mille: u32,
    pub placed_at: u64,
}

// ---------------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------------

/// Recursively key-sorted JSON.
///
/// ★★ `serde_json` here runs with `preserve_order`, so two authors of the same
/// artifact can produce different byte orders. Without this, a package that
/// round-tripped through a peer would read as **altered** on arrival — the
/// integrity check would fire on a difference that is not one.
pub fn canonical(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let sorted: BTreeMap<&String, &Value> = m.iter().collect();
            let mut out = Map::new();
            for (k, val) in sorted {
                out.insert(k.clone(), canonical(val));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

pub fn content_hash(spec: &Value) -> String {
    let bytes = serde_json::to_vec(&canonical(spec)).unwrap_or_default();
    let digest = Sha256::digest(&bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn signed_message(content_hash: &str) -> String {
    format!("{SIGN_DOMAIN}|{content_hash}")
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

pub struct Arena {
    root: PathBuf,
    packages: Mutex<Vec<Package>>,
    installs: Mutex<Vec<Install>>,
    orders: Mutex<Vec<Order>>,
}

impl Arena {
    pub fn at(root: &Path) -> Arena {
        let arena = Arena {
            root: root.to_path_buf(),
            packages: Mutex::new(Vec::new()),
            installs: Mutex::new(Vec::new()),
            orders: Mutex::new(Vec::new()),
        };
        *arena.packages.lock().expect("packages lock") = read_lines(&arena.packages_path());
        *arena.installs.lock().expect("installs lock") = read_lines(&arena.installs_path());
        *arena.orders.lock().expect("orders lock") = read_lines(&arena.orders_path());
        arena
    }

    fn packages_path(&self) -> PathBuf {
        self.root.join("packages.jsonl")
    }

    fn installs_path(&self) -> PathBuf {
        self.root.join("installs.jsonl")
    }

    fn orders_path(&self) -> PathBuf {
        self.root.join("orders.jsonl")
    }

    pub fn orders(&self) -> Vec<Order> {
        self.orders.lock().expect("orders lock").clone()
    }

    pub fn record_order(&self, order: Order) -> Result<(), String> {
        append(&self.orders_path(), &order)?;
        self.orders.lock().expect("orders lock").push(order);
        Ok(())
    }

    pub fn all(&self) -> Vec<Package> {
        self.packages.lock().expect("packages lock").clone()
    }

    pub fn get(&self, id: &str) -> Option<Package> {
        self.packages.lock().expect("packages lock").iter().find(|p| p.id == id).cloned()
    }

    pub fn installs(&self) -> Vec<Install> {
        self.installs.lock().expect("installs lock").clone()
    }

    /// Store a package. ★ The caller has already run the gate — this is the
    /// write, not the decision.
    pub fn record(&self, package: Package) -> Result<(), String> {
        append(&self.packages_path(), &package)?;
        self.packages.lock().expect("packages lock").push(package);
        Ok(())
    }

    pub fn record_install(&self, install: Install) -> Result<(), String> {
        append(&self.installs_path(), &install)?;
        self.installs.lock().expect("installs lock").push(install);
        Ok(())
    }

    /// Every widget installed into one Sustain, as declarations.
    ///
    /// ★★ Read back through the same `AuthoredWidget` shape they were
    /// published in, and **silently dropping an undecodable one would be the
    /// wrong kind of quiet** — so a spec that no longer decodes is skipped and
    /// counted, which the caller reports.
    pub fn widgets_for(&self, sustain_id: &str) -> (Vec<crate::widgets::AuthoredWidget>, usize) {
        let installs = self.installs();
        let mut out = Vec::new();
        let mut undecodable = 0;
        for i in installs.iter().filter(|i| {
            i.kind == Kind::Widget && i.into.as_deref() == Some(sustain_id)
        }) {
            match self.get(&i.package_id) {
                Some(p) => match serde_json::from_value(p.spec.clone()) {
                    Ok(w) => out.push(w),
                    Err(_) => undecodable += 1,
                },
                None => undecodable += 1,
            }
        }
        (out, undecodable)
    }
}

fn read_lines<T: for<'de> Deserialize<'de>>(path: &Path) -> Vec<T> {
    let Ok(file) = std::fs::File::open(path) else { return Vec::new() };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect()
}

fn append<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let line = serde_json::to_string(value).map_err(|e| e.to_string())?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    f.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    f.write_all(b"\n").map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_hash_does_not_care_about_key_order() {
        // ★★ The property that stops a peer round-trip reading as tampering.
        let a = json!({"alpha": 1, "beta": {"x": 1, "y": 2}});
        let b = json!({"beta": {"y": 2, "x": 1}, "alpha": 1});
        assert_eq!(content_hash(&a), content_hash(&b));
    }

    #[test]
    fn the_hash_does_care_about_content() {
        let a = json!({"alpha": 1});
        let b = json!({"alpha": 2});
        assert_ne!(content_hash(&a), content_hash(&b));
    }

    #[test]
    fn array_order_is_content_not_formatting() {
        // ★ Deliberately NOT sorted: `[1,2]` and `[2,1]` are different values,
        //   and a canonicaliser that sorted arrays would erase a real
        //   difference — the opposite of what integrity is for.
        assert_ne!(content_hash(&json!([1, 2])), content_hash(&json!([2, 1])));
    }

    fn package(spec: Value, hash: Option<&str>, sig: Option<&str>, author: &str) -> Package {
        let content_hash = hash.map(str::to_string).unwrap_or_else(|| super::content_hash(&spec));
        Package {
            id: "p1".into(),
            name: "garden".into(),
            kind: Kind::Definition,
            version: "1.0.0".into(),
            description: String::new(),
            tags: vec![],
            spec,
            author: author.into(),
            author_handle: "someone".into(),
            content_hash,
            signature: sig.map(str::to_string),
            origin: Origin::Authored,
            per_mille: 0,
            published_at: 0,
        }
    }

    #[test]
    fn a_spec_edited_after_publishing_reads_as_altered() {
        // ★★★ The reason the hash is recomputed rather than trusted: someone
        //     hand-edits `packages.jsonl` and the record still claims the old
        //     hash.
        let p = package(json!({"a": 1}), Some("deadbeef"), None, &"aa".repeat(32));
        let prov = p.provenance();
        assert!(matches!(prov.integrity, Integrity::Altered { .. }));
        assert!(!prov.safe_to_install());
    }

    #[test]
    fn an_unpublished_local_package_is_unsigned_not_forged() {
        let p = package(json!({"a": 1}), None, None, &"aa".repeat(32));
        let prov = p.provenance();
        assert_eq!(prov.integrity, Integrity::Intact);
        assert_eq!(prov.authenticity, Authenticity::Unsigned);
        // ★★ And therefore installable: refusing it would make the local case
        //    impossible, which is not what a signature is for.
        assert!(prov.safe_to_install());
    }

    #[test]
    fn a_signature_from_the_wrong_key_is_forged() {
        let spec = json!({"a": 1});
        let hash = content_hash(&spec);
        // A syntactically fine signature that verifies against nothing.
        let p = package(spec, Some(&hash), Some(&"11".repeat(64)), &"aa".repeat(32));
        assert_eq!(p.provenance().authenticity, Authenticity::Forged);
        assert!(!p.provenance().safe_to_install());
    }
}
