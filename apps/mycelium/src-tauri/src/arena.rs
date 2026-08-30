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

// ---------------------------------------------------------------------------
// The bundled shelf
// ---------------------------------------------------------------------------

/// Put what ships with the app onto the shelf, once.
///
/// ★★★ **The Library read empty and it was telling the truth about the wrong
/// question.** It is the Arena: the shelf of things somebody *published*. Nobody
/// had, so it showed nothing — while the app shipped with three definitions and
/// a set of curated cards a person could genuinely install. The shelf was empty
/// of published things and the app was not empty of things.
///
/// ★★★ **These are `Bundled`, and that is the whole design.** [`Origin`] has
/// carried a `Bundled` variant — *"shipped with the app"* — since the package
/// type was written, and nothing had ever constructed one. Publishing the
/// built-ins under the household's own key would have been the easy way to fill
/// the screen and it would have been a **lie about authorship**: Bonnie did not
/// write the habitat template. The Arena's entire value is that its three
/// questions stay separate and answerable, and the first one is *who made this*.
///
/// ★★★ **So they are UNSIGNED, on purpose.** `Authenticity::Unsigned` is the
/// honest answer for an artifact this node did not sign, and the provenance
/// panel already renders it as its own fact rather than a failure. A signature
/// from this node's key would say *this household vouches for these bytes*,
/// which is a different claim from *these bytes shipped with the app* — and the
/// second is the true one.
///
/// ★★ **It needs no unlock**, unlike [`World::publish`], and could not use one:
/// there is no key to sign with and nothing to sign. So the shelf is populated
/// at store-open, before anybody types a passphrase, which is also when a person
/// most wants to see what the app can do.
///
/// ★★ **Idempotent by content hash.** A package's id is derived from its spec,
/// so re-seeding an unchanged built-in produces the same id and is skipped.
/// Editing a built-in in a later release produces a different id and a new
/// entry — which is correct: it is a different artifact, and the old one is
/// what an existing install is pinned to.
///
/// Returns how many were newly added.
pub fn seed_bundled(arena: &Arena) -> usize {
    let existing: std::collections::BTreeSet<String> =
        arena.all().into_iter().map(|p| p.id).collect();
    let mut added = 0;

    for pkg in bundled_packages() {
        if existing.contains(&pkg.id) {
            continue;
        }
        // ★ A seed that cannot write is not worth failing a launch for. The
        //   shelf is a convenience; the engine is not.
        if arena.record(pkg).is_ok() {
            added += 1;
        }
    }
    added
}

/// The author field for something nobody here wrote.
///
/// ★★ Not a key, and deliberately not empty: an empty author would render as a
/// blank where a person expects an identity, and blanks get read as bugs. This
/// is a label that says what it is, and `Origin::Bundled` beside it is the part
/// that carries the meaning.
pub const BUNDLED_AUTHOR: &str = "bundled-with-sustena";

/// Everything the app ships that a person could install.
///
/// ★★★ **Cards, and deliberately not the Σ-templates.** The three built-in
/// templates are already installable — the Define screen instantiates them
/// directly — so packaging them would add a second route to the same act, and
/// two ways to do one thing is how a surface starts disagreeing with itself.
/// They are also not cleanly expressible as an `AuthoredDefinition` without
/// reading a `Schema` back out, and a lossy round-trip through the package form
/// would put a *different* definition on the shelf under the same name.
///
/// A card is the opposite case: it maps exactly, and installing one into a
/// Sustain is a real act with no other route.
fn bundled_packages() -> Vec<Package> {
    crate::orchie::widget_declarations()
        .into_iter()
        .filter_map(|w| {
            // ★★ The AUTHORED form, not the core type — the same shape a real
            //    publish carries, so a bundled card and a published one are
            //    indistinguishable to `install` and face the identical gate.
            let authored = crate::widgets::AuthoredWidget {
                id: w.id.clone(),
                render: w.render.clone(),
                inputs: w.inputs.clone(),
                emits: w.emits.clone(),
                event_class: match &w.binding {
                    sustena_core::BindingKey::Unit => None,
                    sustena_core::BindingKey::Event(name) => Some(name.clone()),
                },
            };
            let spec = serde_json::to_value(&authored).ok()?;
            Some(bundled(
                &w.id,
                Kind::Widget,
                "A curated-UI card that ships with the app.",
                vec!["card".into(), "built-in".into()],
                spec,
            ))
        })
        .collect()
}

/// One bundled package, stamped the way an authored one is minus the signature.
fn bundled(
    name: &str,
    kind: Kind,
    description: &str,
    tags: Vec<String>,
    spec: Value,
) -> Package {
    let content_hash = content_hash(&spec);
    Package {
        id: format!("pkg-{}", &content_hash[..12]),
        name: name.to_string(),
        kind,
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: description.to_string(),
        tags,
        spec,
        author: BUNDLED_AUTHOR.to_string(),
        author_handle: "Sustena".to_string(),
        // ★★★ None, and it must stay None. See `seed_bundled`'s note: a
        //     signature here would claim this node vouches for bytes it did not
        //     write. `Authenticity::Unsigned` is the true answer.
        signature: None,
        content_hash,
        origin: Origin::Bundled,
        per_mille: 0,
        // ★★ Zero, not a clock read. A bundled package has no publication
        //    moment — it was in the binary before this node existed — and
        //    stamping "now" would make the same artifact look newly published
        //    on every fresh install.
        published_at: 0,
    }
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

#[cfg(test)]
mod bundled_tests {
    use super::*;
    use sustena_core::{Authenticity, Integrity};

    /// ★ Same convention as `peers_test`: a named directory under the system
    /// temp, cleared first, rather than a dev-dependency for one helper.
    fn arena(name: &str) -> Arena {
        let dir = std::env::temp_dir().join(format!("mycelium-bundled-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        Arena::at(&dir)
    }

    #[test]
    fn the_shelf_is_not_empty_after_seeding() {
        // The whole point: the Library read empty while the app shipped with
        // cards a person could install.
        let a = arena("empty");
        assert_eq!(a.all().len(), 0, "nothing before");
        let n = seed_bundled(&a);
        assert!(n > 0, "the app ships at least one card");
        assert_eq!(a.all().len(), n);
    }

    #[test]
    fn nothing_bundled_claims_this_node_wrote_it() {
        // ★★★ The property that matters more than the shelf being full.
        //     Publishing built-ins under the household's key would have been
        //     the easy way to fill the screen and a lie about authorship.
        let a = arena("author");
        seed_bundled(&a);
        for p in a.all() {
            assert_eq!(p.origin, Origin::Bundled, "{} must be Bundled", p.name);
            assert_eq!(p.author, BUNDLED_AUTHOR, "{} must not carry a node key", p.name);
            assert!(p.signature.is_none(), "{} must be unsigned", p.name);
        }
    }

    #[test]
    fn unsigned_is_reported_as_its_own_fact_and_not_as_a_failure() {
        // ★★ Unsigned and Forged are different answers. A bundled card has
        //    nobody vouching for it, which is true and is not tampering.
        let a = arena("unsigned");
        seed_bundled(&a);
        let p = a.all().into_iter().next().expect("a card");
        let prov = p.provenance();
        assert_eq!(prov.authenticity, Authenticity::Unsigned);
        assert_eq!(prov.integrity, Integrity::Intact, "the bytes are still the bytes");
    }

    #[test]
    fn seeding_twice_adds_nothing() {
        // ★★ Idempotent by content hash, so a launch does not grow the shelf.
        let a = arena("twice");
        let first = seed_bundled(&a);
        let second = seed_bundled(&a);
        assert!(first > 0);
        assert_eq!(second, 0, "the second launch adds nothing");
        assert_eq!(a.all().len(), first);
    }

    #[test]
    fn a_bundled_card_carries_the_same_spec_shape_a_published_one_would() {
        // ★★★ So `install` cannot tell them apart and both face the identical
        //     gate. A bundled artifact that took a shortcut past the check
        //     would be the one thing this shelf must not introduce.
        let a = arena("shape");
        seed_bundled(&a);
        for p in a.all() {
            assert_eq!(p.kind, Kind::Widget);
            let decoded: Result<crate::widgets::AuthoredWidget, _> =
                serde_json::from_value(p.spec.clone());
            assert!(decoded.is_ok(), "{} must decode as the authored form", p.name);
        }
    }

    #[test]
    fn the_content_hash_is_over_the_spec_it_actually_carries() {
        let a = arena("hash");
        seed_bundled(&a);
        for p in a.all() {
            assert_eq!(p.content_hash, content_hash(&p.spec), "{}", p.name);
        }
    }

    #[test]
    fn a_bundled_package_has_no_publication_moment() {
        // ★★ Zero rather than a clock read: it was in the binary before this
        //    node existed, and "now" would make it look newly published on
        //    every fresh install.
        let a = arena("moment");
        seed_bundled(&a);
        assert!(a.all().iter().all(|p| p.published_at == 0));
    }

    #[test]
    fn every_shipped_card_reaches_the_shelf() {
        // ★ One package per card, not one for the set, so a household installs
        //   the ones it wants.
        let a = arena("every");
        seed_bundled(&a);
        assert_eq!(a.all().len(), crate::orchie::widget_declarations().len());
    }
}
