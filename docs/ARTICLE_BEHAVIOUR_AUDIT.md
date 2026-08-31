# Does the running app do what the articles say?

*31 Aug 2026. Behaviour, not code-presence — the question for every row is what
the software **does**, not whether a module with the right name exists.*

## How to read this, and what it is worth

Three verdicts, and the difference between them matters more than the tally:

- **LIVE** — I ran it and watched the result. Real processes, real sockets, real
  files. Evidence is quoted.
- **TEST** — a test exercises the real behaviour end to end, but I did not watch
  it happen outside the harness.
- **SURFACE UNVERIFIED** — the engine behaviour is LIVE or TEST, but whether the
  *screen* does the right thing with it I did not see tonight.

**The honest limit on this whole document.** Both apps are Tauri, which means
the only way to watch a pixel change is to drive the GUI, and computer-use is
not available in this run mode. There is no browser adapter either — the
frontend talks over Tauri IPC, so "open the web build" is not a route that
exists without writing a mock engine, which would prove the wiring and not the
engine. So: **every engine claim below is LIVE or TEST; every claim about what a
person SEES is SURFACE UNVERIFIED unless it says otherwise.** I would rather
hand over that distinction than a page of green ticks.

---

## The eighteen

| # | Module | verdict | what was actually checked |
|---|---|---|---|
| 1 | Sustain | **LIVE** | `two_nodes` opens a real household from disk, folds it, reports it, and reopens it in a second process |
| 2 | Operator | **LIVE** | `budget.record_income` admitted and committed in the live run; the balance moved on disk |
| 3 | Constraint | TEST | 22 region tests; the gate refuses and the refusal carries a reason |
| 4 | Events and Time | **LIVE** | Lamport order drives the fold — both nodes folded the same four entries to the same state, and the earlier divergence proved the ordering is load-bearing |
| 5 | DSL | TEST | 10 predicate tests; parsing and evaluation, no `eval` anywhere |
| 6 | Editing | TEST | 26 editing tests; authoring and the migration predicate |
| 7 | Ingest | TEST | 19 transducer tests + the field reader + correction learning that teaches a shape, not a sentence |
| 8 | Immune | TEST | 13 tests; the secret pre-gate refuses before persistence, so an OTP is never stored |
| 9 | Monitor | TEST / SURFACE UNVERIFIED | 27 tests compute the readings; whether the screen re-renders is now wired (§III) but unwatched |
| 10 | Controller | TEST | 26 tests |
| 11 | Tenet | TEST | 16 tests |
| 12 | Operative | TEST | 16 tests |
| 13 | Curated UI | TEST / SURFACE UNVERIFIED | 45 tests rank and compose; the two UI laws are enforced in layout, and one real clipping bug was found and fixed |
| 14 | **Multiparty** | **LIVE** | the night's main work — §VI and §III below |
| 15 | Mycelium | **LIVE** | two nodes, two OS processes, real sockets, stable ports, unattended start |
| 16 | Arena | TEST | the shelf seeds as `Bundled` and unsigned; peer offers exclude what a node did not write |
| 17 | Pawa | TEST | 13 tests; metered per admitted call |
| 18 | Capstone | — | the composition of the rest; nothing separate to check |

**Counts, so the shape is visible rather than flattering:** 5 LIVE, 11 TEST,
2 of those additionally SURFACE UNVERIFIED, 1 not separately checkable.

---

## §VI — Convergence of shared state. **LIVE.**

The article: merges take the least upper bound, commutative/associative/
idempotent, and *"the holon invariant — never erase the node — is not a policy
sitting on top of the merge. It IS the merge."*

**Two bugs, both found by running it.**

`ReplaceRoot` was an overwrite, so two nodes that had each created the same
Sustain put two genesis lines in one log and the later erased the earlier. And
`World::open` folded in **file** order while `reload` folded causally, so two
nodes converged on the wire and then disagreed on the next open.

Both fixed. Genesis now writes `JoinRoot` (the §VI join); `ReplaceRoot` stays a
replacement because `diff` needs it to say *this key is gone*, which a monotone
join may never do — two jobs, two mutations.

```
prepared
  A key=5e9b3b… port=39951 balance=300.0 log=2
  B key=55ccfd… port=39952 balance=700.0 log=2      <- twin genesis, one id
[B] sync 1: received 2 sent 2  -> log 4
[B] sync 2: received 0 sent 0  (idempotent: true)
[B] rebuild_state == get_state: true
A 300.0  B 300.0  ->  CONVERGED IDENTICAL
```

Both directions, idempotent re-sync, reproducible fold, identical state on both
nodes **after a restart**.

## §III — The relayed, refractory signal. Engine LIVE, surface unverified.

The article: *"each element rests, fires when driven past threshold, then goes
briefly refractory so the excitation moves outward instead of sloshing back into
the elements that just fired."*

Local commits already pushed. What did not exist was a pulse for state that
**arrived** — a peer's write has no local cause, so nothing told any surface,
which is the whole of "close it and reopen it". There is now a typed `Merged`
event carrying the folded state, relayed after the fold and not at all when
nothing arrived, and a refractory state machine (`lib/pulse.ts`) for the five
screens that ask the engine questions instead of reading the store.

Engine side is LIVE (the pulse is built from post-fold state and equals what the
peer holds; a no-op sync relays nothing). **The screen re-rendering is not
something I watched**, for the reason at the top.

## The two UI laws — one real fix, found on hardware

`flex-shrink: 0` on a card was scoped to below `md`, on the reasoning that only
the stacked column made a card shrinkable. It does not: the column is a flex
column at every width. On the desktop Network screen an expanded peer was
compressed until its "shared with this peer" chips were clipped and the card
below drew over them — the same bug the file already documents at 375px, one
breakpoint up, unnoticed because it takes a tall card to show.

Fixed at the token, so every card at every width. **SURFACE UNVERIFIED** that
the specific screen now looks right.

## What is genuinely unbuilt or unproven, plainly

- **The phone.** Nothing tonight ran on Bonnie's device — it dropped off USB
  earlier and his identity is sealed under a passphrase I do not have.
- **The two real devices converging.** Still the open item. The engine bugs that
  blocked it are fixed and proven between two processes; the last mile needs his
  passphrase once.
- **Every screen-level claim.** See the limit at the top.
