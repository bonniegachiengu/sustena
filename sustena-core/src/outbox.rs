//! **Ack, retry, and how deep the queue is** (Ingest · §V·IX).
//!
//! ```text
//!   committed → drop it        the engine has it, durably
//!   rejected  → drop it        the engine looked and said no
//!   unknown   → KEEP it        we do not know, and that is not the same as no
//! ```
//!
//! ★★★ **`Unknown` is the whole reason this module exists.** A request that
//! times out has told you nothing: the engine may have committed it and lost the
//! reply, or never seen it. Treating that as failure drops real transactions;
//! treating it as success loses them just as thoroughly and more quietly. The
//! only honest move is to keep it and send it again — which is safe **only**
//! because capture is idempotent, and that is what idempotency was for.
//!
//! ★★★ **An ack means durably committed, not received.** A server that
//! acknowledges on receipt and crashes before writing has told a phone to forget
//! something that does not exist anywhere. The phone is the only other copy; an
//! ack it acts on has to mean the engine could survive losing power.
//!
//! ## Jitter without randomness
//!
//! ★★★ **The core is deterministic and has no random source**, which turns out
//! to be a better answer than a random one. Jitter derived from the message's
//! own id means two phones retrying the same failure do not thunder together —
//! the usual reason for jitter — *and* the schedule is reproducible, so a
//! support question about why something retried at a particular moment has an
//! answer.
//!
//! ★★ Exponential with a cap. Uncapped, an outbox that has been failing for a
//! day waits a week; without growth, a dead server is hammered by every phone
//! that has ever spoken to it.

/// What the engine said about one capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ack {
    /// **Durably committed.** Safe to forget.
    Committed,
    /// The engine looked at it and refused it. Also safe to forget — sending it
    /// again would get the same answer, and keeping it forever is a queue that
    /// never drains.
    Rejected { reason: String },
    /// ★★★ No answer, or an answer that says nothing. **Keep it.**
    Unknown { detail: String },
}

impl Ack {
    /// May the sender forget this one?
    pub fn settles(&self) -> bool {
        !matches!(self, Self::Unknown { .. })
    }
}

/// One thing waiting to be sent.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    /// Stable across retries — it is what makes them idempotent, and what the
    /// jitter is derived from.
    pub id: String,
    pub attempts: u32,
}

impl Pending {
    pub fn new(id: &str) -> Self {
        Self { id: id.into(), attempts: 0 }
    }
}

/// How long to wait before attempt `n`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Backoff {
    pub base_ms: i64,
    pub cap_ms: i64,
}

impl Backoff {
    pub fn new(base_ms: i64, cap_ms: i64) -> Self {
        Self { base_ms, cap_ms }
    }
}

/// A small, stable hash of an id. Not cryptographic — it only has to spread.
fn spread(id: &str) -> u64 {
    // FNV-1a. Chosen because it is four lines and its behaviour is obvious;
    // nothing here depends on it being hard to reverse.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// **`delay(attempt)`** — exponential, capped, jittered by the id.
///
/// ★★★ Full jitter: the wait is somewhere in `[0, exponential]` rather than
/// exponential-plus-a-wobble. It is the shape that actually spreads a thundering
/// herd, because two senders that back off to the same ceiling still collide if
/// they both wait the ceiling.
///
/// ★★ Attempt 0 waits nothing. The first send should not be delayed by a
/// retry policy it has not yet needed.
pub fn delay_ms(pending: &Pending, backoff: Backoff) -> i64 {
    if pending.attempts == 0 {
        return 0;
    }
    // Saturating, so a pathological attempt count cannot wrap into a short wait.
    let exponent = pending.attempts.min(30);
    let window = backoff
        .base_ms
        .saturating_mul(1_i64.checked_shl(exponent).unwrap_or(i64::MAX))
        .min(backoff.cap_ms)
        .max(1);
    // Deterministic per (id, attempt): the same failure retries on the same
    // schedule every time, and two ids do not line up.
    let mixed = spread(&format!("{}#{}", pending.id, pending.attempts));
    (mixed % window as u64) as i64
}

/// What the sender should do with a capture, given what came back.
#[derive(Debug, Clone, PartialEq)]
pub enum Disposition {
    /// Remove it. The engine has it, or has refused it.
    Forget { because: String },
    /// Keep it and try again after this long.
    Retry { after_ms: i64, attempts: u32 },
}

/// **Apply an ack.**
///
/// ★★★ Only a settled answer removes anything. An unknown outcome leaves the
/// capture exactly where it was, with one more attempt recorded — which is the
/// at-least-once half of at-least-once ∘ idempotent = exactly-once.
pub fn apply(pending: &Pending, ack: &Ack, backoff: Backoff) -> Disposition {
    match ack {
        Ack::Committed => Disposition::Forget { because: "the engine has it, durably".into() },
        Ack::Rejected { reason } => {
            Disposition::Forget { because: format!("the engine refused it: {reason}") }
        }
        Ack::Unknown { .. } => {
            let next = Pending { id: pending.id.clone(), attempts: pending.attempts + 1 };
            Disposition::Retry { after_ms: delay_ms(&next, backoff), attempts: next.attempts }
        }
    }
}

/// **How deep the queue is** — ING-9's leading health metric.
///
/// ★★★ It leads because it moves *before* anything else does. A capture path
/// that has stopped delivering shows here on the first failed send; staleness
/// cannot show until a whole expected interval has passed, and a missing
/// transaction shows only when somebody goes looking for it.
pub fn queue_depth(pending: &[Pending]) -> usize {
    pending.len()
}

/// The one that has been waiting longest, by attempts.
///
/// ★★ Attempts rather than age, because age says how long ago it arrived and
/// attempts say how hard it has been to deliver. A capture on its ninth try is
/// the one that says something is wrong.
pub fn most_attempted(pending: &[Pending]) -> Option<&Pending> {
    pending.iter().max_by_key(|p| p.attempts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backoff() -> Backoff {
        Backoff::new(1_000, 60_000)
    }

    #[test]
    fn a_committed_capture_is_forgotten() {
        let d = apply(&Pending::new("a"), &Ack::Committed, backoff());
        assert!(matches!(d, Disposition::Forget { .. }));
    }

    #[test]
    fn a_refused_capture_is_also_forgotten() {
        // ★★ Sending it again gets the same answer, and keeping it forever is a
        //    queue that never drains.
        let d = apply(
            &Pending::new("a"),
            &Ack::Rejected { reason: "a secret".into() },
            backoff(),
        );
        match d {
            Disposition::Forget { because } => assert!(because.contains("refused")),
            other => panic!("expected forget: {other:?}"),
        }
    }

    #[test]
    fn an_unknown_outcome_is_kept_because_it_is_not_a_no() {
        // ★★★ A timeout has told you nothing. Treating it as failure drops real
        //     transactions; treating it as success loses them more quietly.
        let d = apply(
            &Pending::new("a"),
            &Ack::Unknown { detail: "timed out".into() },
            backoff(),
        );
        assert!(matches!(d, Disposition::Retry { attempts: 1, .. }));
        assert!(!Ack::Unknown { detail: String::new() }.settles());
    }

    #[test]
    fn the_first_send_is_not_delayed_by_a_policy_it_has_not_needed() {
        assert_eq!(delay_ms(&Pending::new("a"), backoff()), 0);
    }

    #[test]
    fn waiting_grows_with_failure_and_stops_growing_at_the_cap() {
        // ★★ Uncapped, an outbox failing for a day waits a week. Without
        //    growth, a dead server is hammered by every phone that ever spoke
        //    to it.
        let long = Pending { id: "a".into(), attempts: 25 };
        assert!(delay_ms(&long, backoff()) <= backoff().cap_ms);
        let absurd = Pending { id: "a".into(), attempts: u32::MAX };
        let d = delay_ms(&absurd, backoff());
        assert!((0..=backoff().cap_ms).contains(&d), "no wrap into a short wait: {d}");
    }

    #[test]
    fn two_captures_failing_together_do_not_retry_together() {
        // ★★★ The actual purpose of jitter. Two senders that back off to the
        //     same ceiling still collide if they both wait the ceiling.
        let a = Pending { id: "message-a".into(), attempts: 4 };
        let b = Pending { id: "message-b".into(), attempts: 4 };
        assert_ne!(delay_ms(&a, backoff()), delay_ms(&b, backoff()));
    }

    #[test]
    fn the_same_failure_retries_on_the_same_schedule_every_time() {
        // ★★★ The core has no random source, and that is a better answer than a
        //     random one: a support question about why something retried at a
        //     particular moment has an answer.
        let p = Pending { id: "message-a".into(), attempts: 3 };
        assert_eq!(delay_ms(&p, backoff()), delay_ms(&p, backoff()));
    }

    #[test]
    fn a_repeatedly_failing_capture_keeps_its_identity() {
        // ★★★ Which is what makes the retry idempotent. A new id per attempt
        //     would turn at-least-once into as-many-times-as-it-failed.
        let mut p = Pending::new("stable");
        for _ in 0..5 {
            match apply(&p, &Ack::Unknown { detail: "down".into() }, backoff()) {
                Disposition::Retry { attempts, .. } => {
                    p = Pending { id: p.id.clone(), attempts };
                }
                other => panic!("expected retry: {other:?}"),
            }
        }
        assert_eq!(p.id, "stable");
        assert_eq!(p.attempts, 5);
    }

    #[test]
    fn queue_depth_moves_before_anything_else_does() {
        // ★★★ Why it is the LEADING metric. Staleness cannot show until a whole
        //     expected interval has passed; a missing transaction shows only
        //     when somebody goes looking.
        let waiting = vec![Pending::new("a"), Pending::new("b")];
        assert_eq!(queue_depth(&waiting), 2);
        assert_eq!(queue_depth(&[]), 0);
    }

    #[test]
    fn the_one_that_has_been_hardest_to_deliver_is_findable() {
        // ★★ Attempts rather than age: age says how long ago it arrived,
        //    attempts say how hard it has been to deliver.
        let waiting = vec![
            Pending { id: "easy".into(), attempts: 1 },
            Pending { id: "stuck".into(), attempts: 9 },
        ];
        assert_eq!(most_attempted(&waiting).unwrap().id, "stuck");
        assert!(most_attempted(&[]).is_none());
    }

    #[test]
    fn the_delay_never_exceeds_the_window_it_was_drawn_from() {
        // ★★ Full jitter means somewhere in [0, exponential]; a delay outside
        //    that would be a schedule nobody declared.
        for attempts in 1..12 {
            let p = Pending { id: format!("m{attempts}"), attempts };
            let d = delay_ms(&p, backoff());
            assert!((0..=backoff().cap_ms).contains(&d), "attempt {attempts} gave {d}");
        }
    }
}
