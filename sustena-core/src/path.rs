//! Dot-path grammar — `finances.liquid.balance`, `staff.roster[0].name`.
//!
//! Hand-written, matching Python's `StateAccessor._SEGMENT_RE`
//! (`(\w+)(?:\[(\d+)\])?` applied per dot-separated segment) rather than
//! pulling in a regex crate: the grammar is tiny, and a dependency-free core
//! is easier to ship to Android, iOS and WASM.
//!
//! Parity note: Python's `\w` is Unicode-aware, so `is_alphanumeric() || '_'`
//! is used here rather than an ASCII-only check. A key like `café` behaves the
//! same in both engines.

use crate::error::{StateError, StateResult};

/// One path segment: a name, optionally indexed (`roster[0]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub name: String,
    pub index: Option<usize>,
}

/// Parse one segment. Returns None if it does not match the grammar, so
/// callers can produce Python's exact error text for the position it failed at.
pub fn parse_segment(raw: &str) -> Option<Segment> {
    if raw.is_empty() {
        return None;
    }
    let bytes: Vec<char> = raw.chars().collect();
    let mut i = 0;

    // name = \w+
    let start = i;
    while i < bytes.len() && (bytes[i].is_alphanumeric() || bytes[i] == '_') {
        i += 1;
    }
    if i == start {
        return None;
    }
    let name: String = bytes[start..i].iter().collect();

    if i == bytes.len() {
        return Some(Segment { name, index: None });
    }

    // optional [digits]
    if bytes[i] != '[' {
        return None;
    }
    i += 1;
    let dstart = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == dstart || i >= bytes.len() || bytes[i] != ']' {
        return None;
    }
    let index: usize = bytes[dstart..i].iter().collect::<String>().parse().ok()?;
    i += 1;

    // must be fully consumed — `fullmatch` semantics
    if i != bytes.len() {
        return None;
    }
    Some(Segment {
        name,
        index: Some(index),
    })
}

/// Split a dot-path into raw segments. Empty path yields one empty segment,
/// matching Python's `"".split(".") == [""]`.
pub fn split(path: &str) -> Vec<&str> {
    path.split('.').collect()
}

/// Parse every segment, producing Python's error text on the first bad one.
pub fn parse_all(path: &str) -> StateResult<Vec<Segment>> {
    let raw = split(path);
    let last = raw.len() - 1;
    let mut out = Vec::with_capacity(raw.len());
    for (i, seg) in raw.iter().enumerate() {
        match parse_segment(seg) {
            Some(s) => out.push(s),
            None => {
                return Err(StateError::Path(if i == last {
                    format!("Invalid final segment: '{seg}' in '{path}'")
                } else {
                    format!("Invalid path segment: '{seg}' in '{path}'")
                }))
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_name() {
        assert_eq!(
            parse_segment("balance"),
            Some(Segment { name: "balance".into(), index: None })
        );
    }

    #[test]
    fn indexed() {
        assert_eq!(
            parse_segment("roster[0]"),
            Some(Segment { name: "roster".into(), index: Some(0) })
        );
    }

    #[test]
    fn rejects_partial_match() {
        // `fullmatch` semantics: trailing junk is a failure, not a prefix match.
        assert_eq!(parse_segment("roster[0]x"), None);
        assert_eq!(parse_segment("a-b"), None);
        assert_eq!(parse_segment("roster["), None);
        assert_eq!(parse_segment(""), None);
    }

    #[test]
    fn unicode_names_behave_like_python_w() {
        assert!(parse_segment("café").is_some());
    }
}
