//! The State primitive — S in Σ = ⟨B, S, V, T, ⊕⟩.
//!
//! A sustain's state, with typed read/write by dot-path, recording every
//! change as a `Mutation` so the event log can reproduce it exactly.
//!
//! PARITY, NOT TRANSCRIPTION (Phase R1). Observable behaviour matches the
//! Python `StateAccessor` — same results, same errors, same mutation records —
//! but the internals are built clean rather than copied.
//!
//! The most important difference is one the port gets for free:
//!
//!   Python's `get()` returns a LIVE reference into the underlying dict, so
//!   `po["status"] = "DELIVERED"` changes real state while recording nothing.
//!   That single fact caused the fold-fidelity bug in three operators and
//!   needed a reconciler to contain it.
//!
//!   Rust cannot hand out that reference. `get()` borrows immutably; every
//!   write goes through a method that records. The defect is not "fixed" here,
//!   it is *unrepresentable* — which is the strongest form of the guarantee and
//!   a concrete reason the portable core belongs in this language.
//!
//! `reconciled_mutations()` is kept for interface parity and still verified by
//! the conformance vectors; in this engine it always equals `mutations()`.

use serde_json::{Map, Value};

use crate::error::{StateError, StateResult};
use crate::mutation::Mutation;
use crate::path;

#[derive(Debug, Clone)]
pub struct State {
    data: Value,
    original: Value,
    mutations: Vec<Mutation>,
}

impl State {
    /// Build from a JSON object. Non-object input is accepted (Python does not
    /// validate either) and simply behaves as an empty container for pathing.
    pub fn new(data: Value) -> Self {
        Self {
            original: data.clone(),
            data,
            mutations: Vec::new(),
        }
    }

    pub fn empty() -> Self {
        Self::new(Value::Object(Map::new()))
    }

    // ── Reads ────────────────────────────────────────────────────────────────

    /// Read a value, or None if the path does not resolve.
    ///
    /// Python returns a caller-supplied default; callers here use
    /// `get(..).unwrap_or(&default)`, which is the same thing without smuggling
    /// a mutable alias out of the container.
    pub fn get(&self, path: &str) -> Option<&Value> {
        let segments = path::parse_all(path).ok()?;
        let mut node = &self.data;
        for seg in &segments {
            node = node.get(&seg.name)?;
            if let Some(i) = seg.index {
                node = node.get(i)?;
            }
        }
        Some(node)
    }

    /// Read a value, erroring exactly as Python's `get_strict` does.
    pub fn get_strict(&self, path: &str) -> StateResult<&Value> {
        let segments = path::parse_all(path)?;
        let mut node = &self.data;
        for seg in &segments {
            match node.get(&seg.name) {
                Some(v) => node = v,
                None => {
                    return Err(StateError::Path(format!(
                        "Key '{}' not found at path '{}'",
                        seg.name, path
                    )))
                }
            }
            if let Some(i) = seg.index {
                match node.get(i) {
                    Some(v) => node = v,
                    None => {
                        return Err(StateError::Path(format!(
                            "Index [{i}] out of range in '{path}'"
                        )))
                    }
                }
            }
        }
        Ok(node)
    }

    /// True when the path resolves to an existing value (even null).
    pub fn exists(&self, path: &str) -> bool {
        self.get(path).is_some()
    }

    pub fn snapshot(&self) -> Value {
        self.data.clone()
    }

    pub fn original(&self) -> &Value {
        &self.original
    }

    pub fn mutations(&self) -> &[Mutation] {
        &self.mutations
    }

    /// Mutations guaranteed to reproduce this state.
    ///
    /// Always equal to `mutations()` here — see the module note. Kept because
    /// it is the contract the engine's write path depends on, and because the
    /// conformance vectors assert it.
    pub fn reconciled_mutations(&self) -> Vec<Mutation> {
        self.mutations.clone()
    }

    /// Always empty: an unrecorded change cannot be expressed in this engine.
    pub fn unrecorded_changes(&self) -> Vec<Mutation> {
        Vec::new()
    }

    // ── Writes ───────────────────────────────────────────────────────────────

    /// Set a value, creating intermediate objects as needed.
    ///
    /// Mirrors Python's `set()`, including its quirk of creating missing
    /// intermediate dicts (so `set("a.b.c", 1)` works on an empty state).
    pub fn set(&mut self, path: &str, value: Value) -> StateResult<()> {
        let segments = path::parse_all(path)?;
        let (last, parents) = segments.split_last().expect("split() yields >=1 segment");

        let mut node = &mut self.data;
        for seg in parents {
            if !node.is_object() {
                return Err(StateError::Path(format!(
                    "Invalid segment '{}' in '{}'",
                    seg.name, path
                )));
            }
            let map = node.as_object_mut().unwrap();
            node = map
                .entry(seg.name.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(i) = seg.index {
                node = node.get_mut(i).ok_or_else(|| {
                    StateError::Path(format!("Index [{i}] out of range in '{path}'"))
                })?;
            }
        }

        let map = node.as_object_mut().ok_or_else(|| {
            StateError::Path(format!("Invalid final segment '{}' in '{}'", last.name, path))
        })?;

        let old = match last.index {
            None => map.insert(last.name.clone(), value.clone()).unwrap_or(Value::Null),
            Some(i) => {
                let arr = map
                    .get_mut(&last.name)
                    .and_then(|v| v.as_array_mut())
                    .ok_or_else(|| {
                        StateError::Path(format!("Path not found: '{}' in '{}'", last.name, path))
                    })?;
                // Python records None when the index is out of range, then
                // raises on the assignment. Preserve the ordering of effects.
                let previous = arr.get(i).cloned().unwrap_or(Value::Null);
                if i >= arr.len() {
                    return Err(StateError::Path(format!(
                        "Index [{i}] out of range in '{path}'"
                    )));
                }
                arr[i] = value.clone();
                previous
            }
        };

        self.mutations.push(Mutation::Set {
            path: path.to_string(),
            old,
            new: value,
        });
        Ok(())
    }

    /// Add to a numeric value, returning the new value.
    pub fn increment(&mut self, path: &str, delta: f64) -> StateResult<f64> {
        let current = self.numeric_at(path, "increment")?;
        let new_val = current + delta;
        self.set(path, json_number(new_val))?;
        Ok(new_val)
    }

    /// Subtract from a numeric value. Refuses to go negative unless allowed.
    pub fn decrement(&mut self, path: &str, delta: f64, allow_negative: bool) -> StateResult<f64> {
        let current = self.numeric_at(path, "decrement")?;
        let new_val = current - delta;
        if !allow_negative && new_val < 0.0 {
            return Err(StateError::Value(format!(
                "Decrement would make '{path}' negative: {} - {} = {}",
                fmt_num(current),
                fmt_num(delta),
                fmt_num(new_val)
            )));
        }
        self.set(path, json_number(new_val))?;
        Ok(new_val)
    }

    /// Append an item to a list, creating the list if absent.
    ///
    /// `id` is caller-supplied here rather than generated internally. Python
    /// mints a UUID when the item has none, which makes the result
    /// non-reproducible — unacceptable in a core whose whole promise is
    /// reproducibility, and untestable by a conformance vector. Id generation
    /// belongs to the caller that owns a clock and a random source; the core
    /// stays deterministic.
    pub fn append(&mut self, path: &str, item: Value, id: &str) -> StateResult<String> {
        let mut item = item;
        if let Some(map) = item.as_object_mut() {
            map.entry("id".to_string())
                .or_insert_with(|| Value::String(id.to_string()));
        }
        let item_id = item
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or(id)
            .to_string();

        if self.get(path).is_none() {
            self.set(path, Value::Array(vec![]))?;
            // Python's own append() records this list creation as a `set`
            // before the append; keeping it means a fold replays identically.
        }

        let target = self
            .get(path)
            .ok_or_else(|| StateError::Path(format!("Path not found: '{path}'")))?;
        if !target.is_array() {
            return Err(StateError::Value(format!(
                "'{path}' is not a list — cannot append"
            )));
        }

        let mut list = target.as_array().unwrap().clone();
        list.push(item.clone());
        self.write_raw(path, Value::Array(list))?;

        self.mutations.push(Mutation::Append {
            path: path.to_string(),
            item_id: item_id.clone(),
            item,
        });
        Ok(item_id)
    }

    /// Remove a list item by its `id` field.
    pub fn remove(&mut self, path: &str, item_id: &str) -> StateResult<()> {
        let target = self.get_strict(path)?;
        let list = target.as_array().ok_or_else(|| {
            StateError::Value(format!("'{path}' is not a list — cannot remove"))
        })?;

        let before = list.len();
        let kept: Vec<Value> = list
            .iter()
            .filter(|v| v.get("id").and_then(|i| i.as_str()) != Some(item_id))
            .cloned()
            .collect();

        if kept.len() == before {
            return Err(StateError::Path(format!(
                "Item id='{item_id}' not found in list at '{path}'"
            )));
        }

        self.write_raw(path, Value::Array(kept))?;
        self.mutations.push(Mutation::Remove {
            path: path.to_string(),
            item_id: item_id.to_string(),
        });
        Ok(())
    }

    /// Replace the whole state — the genesis marker's effect.
    pub fn replace_root(&mut self, value: Value) {
        self.data = value;
    }

    // ── Internals ────────────────────────────────────────────────────────────

    /// Write without recording. Used by append/remove, which record their own,
    /// richer mutation so the fold replays the intent rather than a whole-list
    /// overwrite.
    fn write_raw(&mut self, path: &str, value: Value) -> StateResult<()> {
        let taken = std::mem::take(&mut self.mutations);
        let result = self.set(path, value);
        self.mutations = taken;
        result
    }

    fn numeric_at(&self, path: &str, verb: &str) -> StateResult<f64> {
        let current = self.get_strict(path)?;
        match current.as_f64() {
            Some(n) if !current.is_boolean() => Ok(n),
            _ => Err(StateError::Value(format!(
                "Cannot {verb} non-numeric value at '{path}': {}",
                py_repr(current)
            ))),
        }
    }
}

/// Numbers round-trip as integers when they are whole, matching how Python
/// and serde_json both render them, so snapshots compare byte-for-byte.
fn json_number(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        Value::from(v as i64)
    } else {
        Value::from(v)
    }
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Approximate Python's `repr()` for the error strings the vectors compare.
fn py_repr(v: &Value) -> String {
    match v {
        Value::String(s) => format!("'{s}'"),
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        other => other.to_string(),
    }
}
