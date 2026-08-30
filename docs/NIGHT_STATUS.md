# Night status — 30/31 Aug 2026

*Updated as each piece lands. Everything here is claimed only if I verified it
myself; where a claim is test-proven rather than device-proven, it says so.*

**Branches in play** (none merged yet — merges happen only for green,
live-proven work):

| branch | what | state |
|---|---|---|
| `feat/crdt-root-join` | §VI join instead of overwrite | core 2112 / host 268 green |
| `feat/standing-peering` | stable port, auto-listen, auto-reconnect | host 267 green |
| `feat/live-refresh` | merge announcement + findings log | host 258 green |
| `feat/universal-field-shapes` | ingest field reader + correction learning | core 2129 green |
| `feat/sustena-lore` | (separate session — untouched here) | parked |

---

## The one thing I cannot do, and why

**Auto-unlocking Bonnie's existing identity is impossible without his
passphrase.** The private key is sealed under it; nothing can unseal it without
it, which is the point of the design and not a limitation to route around. I
do not have it and did not ask.

So the authorisation lands as: **a "stay unlocked" mode that caches the key
material after ONE manual unlock**, opt-in, off by default, never in a build
anybody else runs. He types it once, and it never stalls again.

For tonight's live two-node proof I use two real instances I enrol myself,
which §VI treats identically — two nodes, two stores, two identities, one
socket.

---

## Progress

### ✅ §VI reconciliation — LIVE-PROVEN, two OS processes

Multiparty §VI: shared state is a join-semilattice, merges take the least upper
bound, "the holon invariant -- never erase the node -- IS the merge."

**Two bugs found, both by running it rather than by reading it.**

1. **`ReplaceRoot` was an overwrite.** Two nodes that each created the same
   Sustain put two genesis lines in one log and the later erased the history
   before it. Now a join (`root_join`), with the three laws checked by the
   article's own `crdt::laws_hold` against the very type the fold uses.
2. **`World::open` folded in FILE order.** Two processes converged on the wire
   and then DISAGREED on the next open -- same entries, two states, which is
   exactly what §VI says cannot happen. `open` now folds causally, using the
   PUBLIC half of the identity so a locked node still folds correctly.

**The live run** (`cargo run --bin two_nodes`), two separate OS processes:

```
prepared
  A key=348763... port=39901 balance=300.0 log=2
  B key=504014... port=39902 balance=700.0 log=2      <- twin genesis, one id

[A] came up unlocked as bg.myc (unlock remembered)     <- nobody present
[A] listening on Some(39901)
[B] sync 1: received 2 sent 2  -> log 4
[peer] gave 2 new entrie(s); [A] merged and re-folded: balance 300.0 -> 700.0
[B] sync 2: received 0 sent 0  (idempotent: true)      <- §VI third law
[B] rebuild_state == get_state: true

after restart:  A balance 700.0   B balance 700.0   IDENTICAL
```

Both directions, idempotent re-sync, `rebuild_state()==get_state()`, and the
same state on both nodes after a restart.

### ✅ Coming up without a person

The passphrase gate is now optional per node: `remember_unlock` caches the
DERIVED key (not the passphrase, so a reused passphrase is not put at risk),
after the passphrase has actually opened the identity. `forget_unlock` deletes
it and the gate is exactly as it was -- the sealed identity is never touched.
Off by default; a node only comes up unlocked if somebody asked it to.

**To re-enable the gate:** delete `local_unlock.key` from the app data
directory, or call `forget_unlock`. Nothing else changes.

**What I could NOT do:** auto-unlock Bonnie's EXISTING identity. Its key is
sealed under his passphrase and nothing can open it without that. He types it
once, ticks remember, and it never asks again.

### ✅ §III Signal liveness — Mycelium wired, backend live-proven

Multiparty §III: *"each element rests, fires when driven past threshold, then
goes briefly refractory so the excitation moves outward instead of sloshing back
into the elements that just fired."* The elements here are the surfaces.

**What was already live**, and I should say so rather than claim credit: local
commits already pushed through `Committed`/`Refused`/`RolledUp`, so six screens
(Constellation, Composition, Profile, Orchie, Console, Simulate) re-rendered on
a local change without asking anything.

**The gap was state that ARRIVED.** A peer's write had no local cause, so
nothing told any surface. That is the whole of "close it and reopen it".

- New typed `Merged` event, through the same generated bindings, carrying the
  folded state so a surface updates from the message rather than re-querying.
- Relayed from the absorber and the reconnect sweep, AFTER the fold.
- `lib/pulse.ts` implements the §III state machine for surfaces that ask the
  engine questions instead of reading the store: `RESTING → EXCITED →
  REFRACTORY`, with missed pulses remembered as ONE. A hundred arriving entries
  are a hundred pulses and exactly one extra refresh.
- Five screens that fetched once and never again are now subscribed:
  **Network, Library, Economy, Ingest, Define**.

Raw evidence, backend:

```
test peers_test::state_that_arrives_relays_a_pulse_carrying_the_folded_state ok
test peers_test::a_sync_that_changed_nothing_relays_no_pulse               ok
host 274 passed; 0 failed; 0 ignored     clippy 0 errors
```

The first asserts the pulse carries the state AFTER the fold and that it equals
what the peer holds; the second is §III's refractory term at the source -- a
sync that moved nothing relays nothing.

**Honest limit on this one.** The frontend half is typechecked and wired at a
single relay point, and `npm run build` is clean, but I have NOT watched a pixel
change tonight. The app is Tauri-IPC only -- there is no browser adapter, so the
"web build in a browser" route does not exist without writing a mock engine,
which would prove the wiring and not the engine. Driving the desktop GUI needs
computer-use, which is unavailable in this run mode. So: backend proven live,
frontend proven by types and by construction.

### ⏳ In progress
- ingest universal parser, classify-in-place, Library, R1 clipping
- behaviour-level audit of all 18 articles
