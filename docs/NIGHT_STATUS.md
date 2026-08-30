# Night status — 30/31 Aug 2026

*Everything here is claimed only if I verified it myself. Where a claim is
test-proven rather than device-proven, it says so.*

---

## The one thing I could not do, and why

**Auto-unlocking Bonnie's existing identity is impossible without his
passphrase.** The private key is sealed under it; nothing opens it without it,
which is the point of the design and not a limitation to route around. I do not
have it and did not ask.

So the authorisation landed as **a "stay unlocked" mode that caches the key
after ONE manual unlock** — opt-in, off by default, never in a build anybody
else runs. He types it once and it never stalls again.

For the live two-node proofs I used two real instances I enrolled myself, which
§VI treats identically: two nodes, two stores, two identities, one socket.

---

## §VI reconciliation — LIVE, two OS processes

Multiparty §VI: shared state is a join-semilattice, merges take the least upper
bound, and *"the holon invariant — never erase the node — is not a policy
sitting on top of the merge. It IS the merge."*

**Three bugs, and only the first was findable by reading.**

1. **`ReplaceRoot` was an overwrite.** Two nodes that each created the same
   Sustain put two genesis lines in one log; the later erased the earlier.
2. **`World::open` folded in FILE order** while `reload` folded causally. Two
   processes converged on the wire and then **disagreed on the next open** —
   same entries, two states, the one thing §VI says cannot happen. This is also
   Bonnie's "reopen shows different state", at the engine level.
3. **`diff` needs `ReplaceRoot` to say *this key is gone***, which a monotone
   join may never do. Caught by the release gate running the conformance
   target my own `--lib` runs had skipped.

So genesis now writes `JoinRoot` (the §VI join) and `ReplaceRoot` stays a
replacement — two jobs, two mutations. The three laws are checked with
`crdt::laws_hold`, the article's own predicate, against the very type the fold
uses.

**Raw output, two separate OS processes:**

```
prepared
  A key=5e9b3b… port=39951 balance=300.0 log=2
  B key=55ccfd… port=39952 balance=700.0 log=2      <- twin genesis, one id

[A] came up unlocked as bg.myc (unlock remembered)   <- nobody present
[A] listening on Some(39951)
[B] sync 1: received 2 sent 2  -> log 4
[peer] gave 2 new entrie(s); [A] merged and re-folded
[B] sync 2: received 0 sent 0  (idempotent: true)    <- §VI third law
[B] rebuild_state == get_state: true

after restart:  A 300.0   B 300.0   CONVERGED IDENTICAL
```

## §III liveness — engine LIVE, surfaces wired by construction

*"Each element rests, fires when driven past threshold, then goes briefly
refractory so the excitation moves outward instead of sloshing back into the
elements that just fired."* The elements are the surfaces.

Local commits already pushed. **State that ARRIVED had no local cause, so
nothing told any surface** — the whole of "close it and reopen it". Now: a typed
`Merged` event carrying the folded state, relayed after the fold and not at all
when nothing arrived; and `lib/pulse.ts`, the refractory half, so a hundred
arriving entries are one extra refresh.

```
test state_that_arrives_relays_a_pulse_carrying_the_folded_state ... ok
test a_sync_that_changed_nothing_relays_no_pulse                 ... ok
```

### Coverage — every screen of both surfaces, read file by file

Two legitimate ways to be live: **store** (reads the reactive world store) and
**pulse** (asks the engine, so subscribes).

| surface | screen | how | live |
|---|---|---|---|
| **Orchie** | whole shell — see the card table below | pulse on `feed` | ✅ |
| Mycelium | Constellation | store | ✅ |
| Mycelium | Monitor | store (`selectedSustain()`) | ✅ |
| Mycelium | Composition | store | ✅ |
| Mycelium | Profile | store | ✅ |
| Mycelium | Console | store | ✅ |
| Mycelium | Simulate | store | ✅ |
| Mycelium | Network | pulse | ✅ |
| Mycelium | Library | pulse | ✅ |
| Mycelium | Economy | pulse | ✅ |
| Mycelium | Ingest | pulse | ✅ |
| Mycelium | Define | pulse | ✅ |
| Mycelium | Panels, Lock | neither, correctly | n/a — chrome and the unlock screen |

Monitor is worth naming: it has no `world.` reference **and** no `onPulse`, so a
first sweep flagged it as dead. It reads `selectedSustain()` from the store —
the same thing by another name. Caught by opening the file, which is the whole
argument for reading over grepping.

### Orchie, per card

**Bonnie's worry was correct and my earlier status was misleading**: the five
screens I had listed are all Mycelium. Orchie was not subscribed at all — its
`feed` refetched only when the person acted (`onChanged` after classifying,
sweeping, filing), which covers what he causes and nothing else.

One line fixes it, and it is one line *because* every card reads that same feed:

| card | binds to | live |
|---|---|---|
| QueueCard (classify queue) | `feed={f()}` | ✅ |
| Classify | `sustain={f().sustainId}` | ✅ |
| AccountsCard | `feed={f()}` | ✅ |
| InventoryCard | `feed={f()}` | ✅ |
| FiledCard | `feed={f()}` | ✅ |
| OwnNumbersCard | `sustainId={f().sustainId}` | ✅ |
| SmsCard | `sustainId={f().sustainId}` | ✅ |
| NetWorthCard | `feed={props.feed}` | ✅ |
| PersonTabCard (tabs) | `feed={props.feed}` | ✅ |
| DeviceCard | `f().device` | ✅ |

`netting` and `moves` are deliberately NOT subscribed: they report what a
user-triggered pass just did, and refreshing them on an unrelated change would
replace an answer about his action with one about somebody else's.

**A rough edge, disclosed rather than smoothed over.** Orchie's own actions now
refresh twice — its existing `onChanged` plus the relayed pulse. The refractory
window collapses them, so the cost is a redundant fetch and not a flicker.
Removing `onChanged` would be tidier and would make Orchie depend on the relay
for its OWN actions, which is a worse failure mode. Left as is, on purpose.

## Standing peering

Port written down (`network.json`, default 9777) and **never silently swapped**
when busy — that fallback was the original bug wearing a hat. Listener
auto-starts from `unlock` AND `enrol`, because holding the key is the only
precondition. Trusted, addressed, sharing peers swept on a timer. The address
offered is the LAN one, not `127.0.0.1`. Plus `set_peer_address`, without which
a peer that only dialled in can never be reached back.

**To restore the passphrase gate:** delete `local_unlock.key` from the app data
directory, or call `forget_unlock`. The sealed identity is never touched.

## R1 clipping — both reports, one root cause

- **Laptop**: an expanded peer compressed until its "shared with this peer"
  chips were clipped and the card below drew over them.
- **Phone**: the Network panel appeared to ignore swipe entirely.

Both are a card squeezed below its content inside a column that should have
scrolled. `flex-shrink: 0` was scoped to below `md`; `column` is a flex column
at EVERY width. Fixed at the shared token. **Not verified** that the phone's
panel now scrolls under a real finger — the phone is off the cable.

## Library populate — traced, not just merged

`World::open` calls `arena::seed_bundled` (world.rs:371); the shelf gets the
shipped cards as `Bundled` and **unsigned** — signing under Bonnie's key would
be a false authorship claim — and `Library.tsx` renders each `origin`, so a
person can see what came with the app. Idempotent by content hash. Peer offers
exclude bundled, because a node offers what it *wrote*.

## Classify-in-place — already built, verified rather than rebuilt

Mycelium's Ingest list opens the SAME `Classify` component the curated feed
uses, in a modal (`Ingest.tsx` imports it from `Orchie.tsx`). One classifier,
several doors. Nothing to build — saying so rather than claiming it.

## Gate

```
core 2138 lib + all targets green      clippy 0
host  282 green, 0 ignored             clippy 0
tsc clean, npm run build clean
```

## Releases

**v1.1.0** — tagged and pushed, `origin/main == local main`, desktop installer
`Mycelium — Sustena_1.1.0_x64-setup.exe` (4,599,486 bytes) and the arm64 APK.

**v1.1.1** — carries the Orchie liveness fix and the Library shelf. Tagged,
pushed, `origin/main == local main`, android `versionCode 1001001`. Staged in
`\dev\sustena-installers` under the user profile:

| file | bytes | built |
|---|---|---|
| `Mycelium-Sustena-1.1.1-x64-setup.exe` | 4,628,663 | 02:06 |
| `Mycelium-Sustena-1.1.1-x64.msi` | 6,901,760 | 02:06 |
| `Mycelium-Orchie-1.1.1-arm64-debug.apk` | 237,799,138 | 02:09 |

**These are the ones to install.** An earlier 1.1.1 setup.exe timestamped 01:57
exists in the build tree from a run I KILLED mid-flight; it was overwritten by
this run, and nothing from that run should be trusted.

`scripts/release.ps1` refused three times before letting anything through — an
uncommitted lockfile twice, and the conformance failure above. The conformance
refusal was the valuable one: it ran a target my own `--lib` runs had skipped.

**One mess worth recording so it is not repeated.** Three release runs ended up
racing on the same target directory, because I started them in the background
and assumed they had finished rather than checking. They corrupted nothing — all
five version manifests agreed — but they roughly tripled the build time and left
the stale 01:57 artifact above. Fixed by stopping all of them, reverting the
half-applied manifests to a clean tree, and running exactly one.

## What is NOT proven

- **No pixel was watched.** Both apps are Tauri; computer-use is unavailable in
  this run mode and there is no browser adapter, so "open the web build" is not
  a route that exists without writing a mock engine — which would prove the
  wiring and not the engine. Every engine claim is LIVE or TEST; every claim
  about what a person SEES is for the morning session.
- **Nothing ran on Bonnie's phone.** It dropped off USB and did not come back.
- **The two real devices converging** is still the open item. The engine bugs
  that blocked it are fixed and proven between two processes; the last mile is
  his passphrase, once.
