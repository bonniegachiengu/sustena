# The two-device push bug: what the evidence eliminates, and what survives

*31 Aug 2026. Code and the committed device trace only. No device was touched.*

## The reading being explained

From his phone, `sync_debug.txt`, written **11:05 today**:

```
sustain=homestead
mine_entries=285
mine_frontier = { d6502ecd…: 283, f86df2ca…: 2 }
theirs        = { d6502ecd…: 283, f86df2ca…: 2 }
received=0
sent=0
```

`mine` is the phone: 283 of its own entries plus the laptop's 2. Its frontier is
therefore correct and unremarkable. **The whole question is `theirs`** — the
frontier the responder put in its `Give`.

## What the code says the responder must send

```rust
// peers.rs, answer_want
let replica = self.store.read_replica(sustain_id, me)?;
let frontier = replica.frontier();
session.send(stream, &Frame::Give { entries, frontier, spec })
```

```rust
// store.rs
pub fn read_replica(&self, sustain_id: &str, node: &str) -> StoreResult<Replica<LoggedEvent>> {
    let mut replica = Replica::new();
    for line in self.read_log(sustain_id)? { replica.insert(entry_of(line, node)); }
    Ok(replica)
}
```

`read_log` reads the file. There is **no cache and no in-memory path** — the
frontier is a pure function of what is on disk. `entry_of` attributes a line
with no `origin` to the reading node at `counter = seq + 1`, so a two-line
legacy log can only ever yield `{ <reader>: 2 }`.

## Eliminated, each with its evidence

| hypothesis | evidence against |
|---|---|
| **A phantom/duplicate install served :9777** | Every scratch root's `network.json` listens on 399xx/98xx. `realphone` — the only root holding a copy of the phone's log — has no `network.json` at all, so it was never opened as a node. |
| **A second data root diverged** | Only two exist: `%APPDATA%\…mycelium` and the Claude-container mirror. Both hold the same 2-line `homestead.jsonl` with identical mtimes. |
| **The phone synced with itself** | `wire.rs:559` refuses a peer presenting this node's own key, and that check has been in since **19 Aug** (946de45) — months before the trace. |
| **The responder echoed the asker's clock** | `frontier: replica.frontier()` has been the only form since **19 Aug** (5e5b16f). `git log -S "frontier: have"` returns nothing. |
| **The running build differed** | `git diff v1.1.1..HEAD` touches `peers.rs` only (+43/−1, my instrumentation). `sync.rs`, `store.rs`, `wire.rs` are byte-identical, so the frontier logic at 11:05 was today's. |
| **The laptop's log was bigger then and was reverted** | `events/` dir mtime is **19 Aug**; `homestead.jsonl` mtime is **29 Aug**. Neither has been written since, so it held 2 entries at 11:05. |

## The contradiction, stated plainly

1. The laptop's log held **2 entries** at 11:05 (mtimes).
2. The laptop's `peers.json` was written at **11:05:06** — the same second as the
   phone's `last_synced` — so the real app was in that exchange.
3. The code can only have sent `{ f86df2ca: 2 }`.
4. The phone recorded `{ d6502ecd: 283, f86df2ca: 2 }`.

These cannot all hold. **I could not resolve it from code and the trace alone**,
and I am not going to pick a story to make it resolve — the last time I did that
I retracted a real observation as a harness artefact and was wrong.

### The one that survives, and why it is weak

`peers.json` is written by `note_sync` on the **dialer** path as well as
`note_contact` on the responder path. So 11:05:06 may record the laptop
*dialling out*, not answering — leaving open that something else answered
:9777. But nothing found on disk could have produced that frontier, so this
explains the timestamp without explaining the number.

## The minimal safe fix: make the claim checkable

Whatever produced it, the failure mode is the same and is the thing worth
fixing: **a responder made a claim about what it holds, the dialer trusted it,
and the result was `sent=0` — which is indistinguishable from healthy
idempotence.** A lie and a success currently look identical.

Add a **holdings digest** alongside the frontier: a hash over the sorted
`(node, counter)` pairs the responder actually holds.

```rust
Frame::Give { sustain_id, entries, frontier, digest, spec }
```

Then in `sync_peer`, when a round ends with `received == 0 && sent == 0` —
the claim of convergence — the dialer compares its own digest with the peer's.
Equal frontiers with unequal digests is **incoherent**, and it becomes a loud
refusal instead of a silent no-op.

* It is additive and behind the existing `PROTOCOL` constant, so it is a
  version bump rather than a redesign.
* It costs one hash per sync over data already in memory.
* It would have caught 11:05 **at the moment it happened**, and named which
  side was wrong — which is the whole reason this took a day to not solve.

**Not shipped.** Recorded for Bonnie's greenlight, with the live sync test.
