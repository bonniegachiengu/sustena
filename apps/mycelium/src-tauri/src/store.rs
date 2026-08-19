//! On-disk persistence — **the event log, not a state snapshot**.
//!
//! ## ★★★ Why the log and not the state
//!
//! The engine's central property is `state = fold(events)`. Persisting a state
//! snapshot would make the file the source of truth and the log a story about
//! it — which is exactly backwards, and the first time the two disagreed there
//! would be no way to say which was right. So what survives a quit is the
//! **append-only log**, and the state is rebuilt by folding it on load, by the
//! same `fold_events` the engine ships.
//!
//! ★★ That makes the property *checkable on every launch* rather than asserted:
//! [`Store::load_sustain`] returns the folded state, and nothing else can.
//!
//! ## The format
//!
//! ```text
//! <app_data_dir>/
//!   sustains.json                  the registry — id, label, template, parent
//!   events/<sustain_id>.jsonl      one LoggedEvent per line, append-only
//! ```
//!
//! ★ Plain JSONL rather than SQLite or sled, and that is a choice with reasons:
//! the log **is** the format, a human can read it, `serde_json` is already a
//! dependency, and there is no schema migration to get wrong. What it gives up
//! is named in the honest-limits section of the app README — no compaction, no
//! concurrent writers, and a torn final line on a hard power loss would need
//! the last record dropping.
//!
//! Each append is followed by `sync_all()`, so a clean quit — the case the
//! acceptance check exercises — cannot lose a committed event.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sustena_core::{
    fold::{fold_events, FoldEvent},
    mutation::Mutation,
    operator::Execution,
};

use crate::dto::EventDto;
use crate::templates::TemplateId;

/// One line of a Sustain's log.
///
/// ★ Carries more than the fold needs — the operator that caused it and the
/// events it published — because an event log a person cannot read is only a
/// database. The fold uses `mutations` alone, so the extra fields can never
/// change what the state is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggedEvent {
    pub seq: u64,
    /// The operator that produced it, or `genesis` for the opening line.
    pub operator: String,
    #[serde(default)]
    pub events: Vec<EventDto>,
    #[serde(default)]
    pub mutations: Vec<Mutation>,
}

impl LoggedEvent {
    /// ★★ The genesis line: the opening state as one `ReplaceRoot`. Every
    /// Sustain's history starts here, so there is no "before the log" state to
    /// special-case.
    pub fn genesis(state: &Value) -> Self {
        LoggedEvent {
            seq: 0,
            operator: "genesis".into(),
            events: Vec::new(),
            mutations: vec![Mutation::ReplaceRoot { value: state.clone() }],
        }
    }

    pub fn of(seq: u64, operator: &str, x: &Execution) -> Self {
        LoggedEvent {
            seq,
            operator: operator.to_string(),
            events: x.events.iter().map(EventDto::from).collect(),
            mutations: x.mutations.clone(),
        }
    }
}

/// What the registry remembers about one Sustain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SustainRecord {
    pub id: String,
    pub label: String,
    pub template: TemplateId,
    /// `⊕` — the parent in the household's composition, if any.
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub sustains: Vec<SustainRecord>,
    /// Which Sustain the cockpit is looking at. Persisted so a relaunch lands
    /// where you left off.
    #[serde(default)]
    pub selected: Option<String>,
}

#[derive(Debug)]
pub enum StoreError {
    Io(String),
    Corrupt { file: String, line: usize, reason: String },
    Fold(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "disk: {e}"),
            // ★ Names the file AND the line. A corrupt-log error that cannot
            //   say where is an alarm, not a diagnosis.
            StoreError::Corrupt { file, line, reason } => {
                write!(f, "{file} line {line} is unreadable: {reason}")
            }
            StoreError::Fold(e) => write!(f, "the log does not replay: {e}"),
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e.to_string())
    }
}

pub type StoreResult<T> = Result<T, StoreError>;

/// The household's durable home.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn at(root: impl Into<PathBuf>) -> StoreResult<Store> {
        let root = root.into();
        fs::create_dir_all(root.join("events"))?;
        Ok(Store { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn registry_path(&self) -> PathBuf {
        self.root.join("sustains.json")
    }

    fn log_path(&self, sustain_id: &str) -> PathBuf {
        // ★ Ids are host-minted (`habitat-bonnie`, `homestead`) and never come
        //   from a user, so there is no path-traversal question to answer —
        //   but the sanitiser is cheap and the day an id becomes user-typed it
        //   is already correct.
        let safe: String = sustain_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        self.root.join("events").join(format!("{safe}.jsonl"))
    }

    // ── the registry ─────────────────────────────────────────────────────────

    pub fn load_registry(&self) -> StoreResult<Registry> {
        let path = self.registry_path();
        if !path.exists() {
            return Ok(Registry::default());
        }
        let text = fs::read_to_string(&path)?;
        serde_json::from_str(&text).map_err(|e| StoreError::Corrupt {
            file: "sustains.json".into(),
            line: e.line(),
            reason: e.to_string(),
        })
    }

    pub fn save_registry(&self, registry: &Registry) -> StoreResult<()> {
        let text = serde_json::to_string_pretty(registry).map_err(|e| StoreError::Io(e.to_string()))?;
        let tmp = self.registry_path().with_extension("json.tmp");
        {
            // ★ Written to a temp file and renamed, so a crash mid-write leaves
            //   the previous registry intact rather than a half one.
            let mut f = File::create(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.sync_all()?;
        }
        fs::rename(&tmp, self.registry_path())?;
        Ok(())
    }

    // ── the log ──────────────────────────────────────────────────────────────

    /// Every line of one Sustain's log, in order.
    pub fn read_log(&self, sustain_id: &str) -> StoreResult<Vec<LoggedEvent>> {
        let path = self.log_path(sustain_id);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&path)?;
        let mut out = Vec::new();
        for (i, line) in BufReader::new(file).lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let ev: LoggedEvent = serde_json::from_str(&line).map_err(|e| StoreError::Corrupt {
                file: format!("events/{sustain_id}.jsonl"),
                line: i + 1,
                reason: e.to_string(),
            })?;
            out.push(ev);
        }
        Ok(out)
    }

    /// Append one event, durably.
    ///
    /// ★★ `sync_all` before returning: a committed event that the app reported
    /// as committed must survive the quit that follows it.
    pub fn append(&self, sustain_id: &str, event: &LoggedEvent) -> StoreResult<()> {
        let line = serde_json::to_string(event).map_err(|e| StoreError::Io(e.to_string()))?;
        let mut f = OpenOptions::new().create(true).append(true).open(self.log_path(sustain_id))?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_all()?;
        Ok(())
    }

    /// ★★★ **`state = fold(events)`, on every load.**
    ///
    /// The state is not stored anywhere. This is the only way to obtain it, so
    /// the property cannot quietly stop being true.
    pub fn load_state(&self, sustain_id: &str) -> StoreResult<(Value, u64)> {
        let log = self.read_log(sustain_id)?;
        let next_seq = log.last().map(|e| e.seq + 1).unwrap_or(0);
        let folded: Vec<FoldEvent> =
            log.into_iter().map(|e| FoldEvent { mutations: e.mutations }).collect();
        let state = fold_events(&folded, None).map_err(|e| StoreError::Fold(format!("{e:?}")))?;
        Ok((state, next_seq))
    }
}
