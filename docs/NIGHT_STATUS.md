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

---

# 31 Aug 03:00–03:40 — the push bug, with real devices

## 2 — Did his data converge? NO. And the laptop's balance is not real.

The laptop's Homestead log, in full, from the running app's own store:

```
seq 0 | genesis                  | origin self
seq 1 | system.backfill_declared | origin self
```

Two events. Every habitat identical. Nothing written since 29 Aug 10:05. **None
of the phone's 285 events crossed.**

Folding a COPY of that store gives the truth the screen does not:

```
lapcopy  log=2  balance=0.0
{"finances":{"liquid":{"balance":0.0},"pockets":{"food":{...0.0}},...}}
```

**The laptop folds to 0.00.** Its UI showed `516,699.48` — a number its own log
cannot back. That is its own bug and the most dangerous one on this page,
because it makes an unconverged node look converged.

`RECEIVED 0 / SENT 0` was NOT idempotence. Computed from both real logs:

```
laptop frontier: {f86df2ca: 2}
phone  frontier: {d6502ecd: 283, f86df2ca: 2}
phone SHOULD send: 283 entries (all d6502ecd-origin)
```

**Sustains 8 vs 7:** the registry holds 7. There is an orphan `garden-1.jsonl`
event log, present as a log and absent from `sustains.json` — the likely eighth
in the UI count.

## 1 — The push bug: what is now RULED OUT, at byte level

**It is not the transport.** A proxy between the two devices, framing on the
wire's own length prefix:

```
--- session from 127.0.0.1:2455 ---
phone->laptop  frame 281      laptop->phone  frame 281   (handshake)
phone->laptop  frame 159      laptop->phone  frame 159
phone->laptop  frame 469      laptop->phone  frame 641
phone->laptop  frame 503      laptop->phone  frame 503
--- session ended ---
```

Largest frame **641 bytes**. A Give carrying 283 entries is ~150 kB. So the
phone genuinely computed nothing to send; nothing was dropped in flight.

**It is not the algorithm, and not his data.** I implanted his REAL logs and
REAL registries into two scratch nodes, origins remapped onto their keys:

```
[B] before: balance 516699.48 log 285
[B] sync 1: received 0 sent 283  -> balance 516699.48
[B] sync 2: received 0 sent 0    (idempotent: true)
[B] rebuild_state == get_state: true
[peer] gave 283 new entrie(s); [A] merged and re-folded: balance 516699.48
```

**Node A — the laptop's shape — received all 283 and folded to his real
household.** With his own data, the code converges.

**A misconfiguration I introduced and then found.** My `adb reverse tcp:9777`
squatted the port the phone's app wants, so it showed "not listening" — v1.1.1
refusing to silently pick another port, correctly. When I freed 9777 the app
took it, and the phone's stored peer address `127.0.0.1:9777` then pointed **at
itself**: a self-sync returns 0/0 with no error, which is exactly what
03:09:16 shows. That explains the LATER syncs. It does not explain 03:06:52,
when the reverse was still in place and the laptop's `peers.json` was rewritten
at 03:07:16 — proof of real contact.

**So the remaining unknown is narrow:** with `theirs = {laptop: 2}`, the phone's
`missing_from` returned empty in the app while returning 283 in a harness fed
the same bytes. Every other explanation is eliminated.

## 3 — Desktop liveness: cause found

The laptop was the RESPONDER. The pulse fires from the absorber (only when
`accept_give` actually wrote entries — here zero, correctly) and from the
reconnect sweep (the initiator path). But the inbound connection DID change the
laptop: `serve` calls `book.seen(...)` and rewrote `peers.json` at 03:07:16.
**Nothing relays a pulse for a peer-book change**, so the Network screen had no
reason to re-render and kept saying "never synced".

§III's relay is wired for state, not for peering. That is my omission.

---

# 31 Aug 03:40–03:50 — I cost the phone its unlock

**An own-goal, recorded plainly.** To load the instrumented build I force-stopped
and relaunched the phone app. The Sustena identity lock does not survive a
restart unless `remember_unlock` was set -- and it was not, because the toggle
is not surfaced in the UI (Bonnie reported that earlier tonight; it is a real
gap I logged and did not fix). So the relaunched app came up LOCKED.

A locked node cannot peer: `reconnect_all` is gated on `is_unlocked()`, so the
sweep stops, the listener does not come up, and no sync runs. That is the design
working. It is also why `sync_debug.txt` was never written -- the instrumentation
is correct and simply never executed.

Evidence:

```
phone app pid 10104 alive
peers.json last_synced 1788136567 = 03:36:07   (the LAST sync, from the OLD build)
sync_debug.txt: No such file or directory
```

The install landed at 03:36:28, AFTER that sync, so the old build's silence
proved nothing either.

**I had flagged this exact risk for the laptop and then did it to the phone.**
The laptop was left alone and is still unlocked and listening on 9777.

**State right now, all fold-backed:**

| | |
|---|---|
| laptop Homestead | 2 events, folds to balance 0.0 |
| phone Homestead | 285 events (data intact through two installs) |
| laptop app | running, unlocked, listening 9777 |
| phone app | running, **locked** -- cannot peer until Bonnie unlocks |
| converged? | **NO** |

**What unblocks it:** Bonnie unlocks the phone once. The instrumented build is
already installed, so the next sweep writes the numbers that settle the push
bug. Nothing else is needed from him.

## What landed while the device was blocked

`fix/fold-invariant-and-peer-liveness` (65be0b5), host 285 green, clippy clean:

- **The anti-faking invariant.** `fold_divergence` asks whether what a screen
  would show still matches the log, WITHOUT re-folding -- `reload` would make
  them equal and report nothing, which is the opposite of an invariant. Two
  tests: one walks every point state can change (local commit, synced-into,
  restart); the other forces a divergence through a test-only door and asserts
  the check SEES it, because a guard that only ever passes is not a guard.
- **Peering changes are changes.** `Peering::edit` relays a pulse for any book
  change, so an inbound connection refreshes the Network screen. Plus
  `last_contact` on the responder, because `last_synced` is initiator-only and
  a node synced INTO reported "never synced" after a completed session.

---

# 31 Aug 04:00–04:40 — device-free work while the push bug is parked

## Stay-unlocked, surfaced — `967ebee`

The toggle existed in the engine and nowhere a person could reach it, which is
what caused the relock own-goal. Now on the LOCK screen on BOTH faces: that is
the one moment somebody is already thinking about the passphrase, and a setting
in preferences is one nobody finds. Applied only AFTER the passphrase genuinely
opened the identity; unticking calls `forget_unlock`, so it is a two-way
control. The caption carries the trade in the label itself.

## Where the record begins — the intake cutoff

Canon: the Ingest paper makes τ total over what crosses the boundary. This
decides what crosses, so it sits IN FRONT of τ rather than filtering after it.
A message older than the start is not classified-and-skipped, it is **not
admitted**: nothing stored, nothing queued, and the classify queue does not open
with two and a half thousand decisions nobody asked for.

Applied at three depths, strongest first:

1. **The Android content query.** `startAtMs` is an INCLUSIVE floor on the SMS
   query itself, so an old text is never read off the phone at all.
2. **`capture_at`**, ahead of even the secret gate — not admitting is strictly
   less than refusing, and costs a parse we do not need.
3. **`IntakeWindow`** in core: the pure decision, with `Admission::Before`
   carrying both timestamps so a surface can say by how much rather than "no".

Decisions worth naming:

- **Open by default.** Somebody who never touches it loses nothing.
- **An undated message is ADMITTED.** Refusing needs certainty, admitting only
  needs doubt; losing a real payment to a missing field is far worse than one
  more question in the queue.
- **The start itself is included.** An exclusive edge would silently drop the
  message a person set the cutoff to catch.
- **`before_start` is counted separately from `refused`.** A person who set a
  date has not refused two thousand messages -- they never asked for them.
- **Moving it never deletes anything.** A boundary that retroactively erased a
  household's record would be worse than the backlog it avoided. Tested.

12 tests (6 core, 6 host). One picker component serving both faces -- on the
phone it sits on the card that reads the texts, on the cockpit beside the
sources, because "what do we listen to" and "from when" are the same question.

Gate: core **2144** green, host **291** green, clippy clean on both, tsc clean,
frontend builds.

## Still parked, unchanged

The push bug waits on Bonnie unlocking the phone once. Neither app was touched.
Fold-backed state is as recorded above: laptop Homestead 2 events folding to
0.0, phone 285, **not converged**.

---

# 31 Aug 10:45–11:00 — the push bug, instrumented on the real devices

Both apps unlocked, both reachable, tunnels up. The instrumented build wrote the
numbers.

```
sustain        = homestead
mine_entries   = 285
mine_frontier  = { d6502ecd…: 283,  f86df2ca…: 2 }
theirs         = { d6502ecd…: 283,  f86df2ca…: 2 }
received = 0     sent = 0
laptop Homestead on disk = 2 events   (both LEGACY, mtime 29 Aug)
```

**`theirs` is byte-identical to the phone's own frontier.** That is the bug in
one line: `missing_from(theirs)` sends entries whose counter exceeds
`theirs.get(node)`, and `theirs` already claims `d6502ecd: 283` -- the phone's
own 283 entries -- so nothing qualifies and `sent = 0` is the CORRECT answer to
a false question. **The push computation was never wrong. It is being told a
lie about what the other side holds**, and my earlier "the push is broken"
reports were measuring the wrong end.

## What I ruled out, each with a reading

| hypothesis | test | result |
|---|---|---|
| transport dropping the payload | proxy framing on the wire's length prefix | largest frame **641 B**; 283 entries would be ~150 kB — nothing was dropped |
| the algorithm | his real logs + registries in two scratch nodes | **sends 283**, node A folds to 516,699.48 |
| loopback via the adb tunnel | re-pointed to the laptop's **LAN** address `192.168.1.66:9777`, no adb, no localhost | **identical result** |
| the laptop never answering | `peers.json` mtime on the laptop | **10:57:04**, seconds after the sync — it served it |

So: the laptop IS the responder, over a path where loopback is impossible, and
it returns a frontier containing `d6502ecd: 283`. Its own log holds two LEGACY
entries, which `entry_of` attributes to the reading node, so
`read_replica("homestead", f86df2…).frontier()` can only be `{f86df2…: 2}`.
`answer_want` sends exactly that — I re-read it.

**The laptop is reporting a frontier its own on-disk log cannot justify.** That
is the open question, and it is now narrow: everything between the two devices
is accounted for except what the laptop computes for its own replica.

## The one thing that would settle it

Instrument the LAPTOP's `answer_want` to record the replica it read and the
frontier it sent. That costs a restart, and a restart relocks it — the toggle
that would prevent this only exists in a build not yet installed there.

**Nothing was written to his household.** Both apps remain unlocked. Fold-backed
state unchanged: laptop 2 events folding to 0.0, phone 285, **not converged**.

---

# 31 Aug, 13:40 — the open question, closed; and a correction I owe

## The correction first

The section above says *"the laptop is reporting a frontier its own on-disk log
cannot justify."* **That reading did not come from the real laptop.** It came
from a scratch node in the harness, and I attributed it to his machine. The
evidence that settles it is on his disk:

```
events/homestead.jsonl   last written  2026-08-29 10:05:49
grep -rl d6502ecd <store>  ->  peers.json ONLY
```

His Homestead log has not been touched in two days, and the phone's node key
appears **only** in the trust record, never in an event. No peer entry has ever
been written to that store. So there was never a mysterious frontier on the real
pair — there was never a merge at all.

## What the instrumentation actually says, now that it has run

Driven with **his real 2-line laptop log** on one side and a real-shaped 285-line
peer log on the other, in two OS processes over a socket:

```
before   A(his real log) = 2 lines      B(peer) = 285 lines
dial 1   A -> 287 lines on disk
dial 2   received=0  sent=0             <- idempotent, as §VI requires
         mine_frontier == theirs == {A:2, B:285}
fold     A balance=555955.56   B balance=555955.56   (identical)
```

**`mine_frontier == theirs` with `sent=0` is not the bug. It is what a second,
correctly idempotent sync looks like.** I had been reading the signature of
success as the signature of failure.

So: transport, push, `missing_from`, `merge_entries` and persistence all work,
proven with his own log as one of the two sides.

## Then why has nothing crossed?

Because the first sync never happened on the real pair, and the reason is
mundane: `peers.json` records the phone with **`"address": null`**, so the
laptop cannot dial it, and the phone's own build (03:36 APK) predates the
standing-peering work that would have it dial the laptop unprompted.

## The unbacked balance

| check | result |
|---|---|
| `516699` anywhere in the laptop store | **not present in any file** |
| laptop Homestead fold | `balance 0.0` |
| laptop log last written | 2026-08-29, before any of this |

The laptop's store cannot produce that number and never held it. I cannot
inspect the phone right now to confirm the screen it came from, so I am not
claiming where it came from — only that **the laptop's store is not it**. The
`fold_divergence` invariant stays regardless: a number on screen that the log
does not support must be impossible to display, not merely unlikely.

## Delivery — fixed and verified from his side

| | |
|---|---|
| path his shortcut opens | `%LOCALAPPDATA%\Mycelium — Sustena\mycelium.exe` |
| before | stale |
| after | **1.1.1, written 13:23:27**, carries `remember_unlock`, `forget_unlock`, `intake_start`, build hash `406d6b8` |
| phone APK | **built 13:35**; install armed, fires the moment the device returns |

`scripts/deploy-apps.ps1` is the permanent mechanism. Running it found four real
faults in itself, all now fixed: a mangled Rust home path, a run that refused
over a step `-PhoneOnly` should not have taken, a staleness rule stated over
clocks instead of content, and native stderr being treated as failure.

## Blocked on one physical thing

The phone is off USB **and** off the LAN (`ping 192.168.1.64` fails). I did not
write a guessed address into his real peer book; the phone's actual address gets
captured when it returns. Nothing was written to his household.

---

# 31 Aug, 14:20 — the proof, on a harness that no longer lies

The earlier run reported `balance=null` on both sides and then panicked, which
read exactly like a broken sync. It was not. `implant` dropped a log into a root
without ever **registering** the household, so nothing knew the Sustain existed
to read it — and an `unwrap()` on that absence panicked *after* a sync that had
already worked. Both fixed; the harness now tells the truth.

Re-run from scratch, his real Homestead log as one side:

```
BEFORE  A = 2 entries (his real laptop log)   B = 286 entries (peer)
[B] sync 1: received 2  sent 286   -> log 288
[B] sync 2: received 0  sent 0     -> log 288   (idempotent: true)
[B] rebuild_state == get_state: true

after reopening both from disk:
  A  log=288  balance=555955.56
  B  log=288  balance=555955.56
```

Two OS processes, real sockets, and the numbers read back **after a restart**,
so this is durability and not a live cache. Both nodes carried their own genesis
into the merge and neither erased the other — §VI's join, on a real household's
log rather than a fixture.

**What this closes.** Transport, `missing_from`, `merge_entries`, persistence,
causal fold and idempotence are all proven on his data. What remains is not a
bug: his phone is off USB and off the LAN, so his actual 285 entries have not
had a route to cross. The moment it returns, the armed install fires and both
sides will hold the standing-peering build.

---

# 31 Aug, 14:40 — §III liveness, per view, both faces

Bonnie's worry was that the fix landed on the cockpit and not on Orchie. The
audit says something more useful than yes or no: **six of fifteen views were
live, and the nine that were not had nothing to do with the screens.**

Monitor, Composition, Constellation and Simulate own no resources — they read
the shared `live` store. So the thing that was deaf to a change a peer wrote was
never those screens; it was the store behind them. One subscription in the shell
makes all four live at once, which is why the fix is three lines rather than
nine files.

| view | §III |
|---|---|
| App (shell, belt, selector) | subscribes directly |
| Orchie | subscribes directly (3 sites) |
| Ingest, Economy, Network, Library, Define | subscribe directly |
| Console | **added** — a package can add operators, and packages arrive from peers |
| Profile | **added** — access follows the peer book, which changes elsewhere |
| Monitor, Composition, Constellation, Simulate | via the shell's store subscription |
| Lock, Panels | no live data — a lock screen and a set of authored absences |

Loop-free by inspection: `refreshWorld` re-reads and merges, it never relays.
Only a local commit and an arrived merge relay, which is the §III excitation
this is the refractory response to.

**Still SURFACE UNVERIFIED**, and I will keep saying so: I have not watched a
pixel change. Driving the GUI is not available to me in this run mode, and I
reported a window as "up at the lock screen" once from a process existing. I do
not intend to repeat that.

---

# 31 Aug, 14:45 — v1.1.2 released, and his laptop is running it

## Verified from his side, not from `target/release`

```
C:\Users\DELL\AppData\Local\Mycelium — Sustena\mycelium.exe
  written 2026-08-31 14:45:03    FileVersion 1.1.2
  carries: build stamp "7fd417c", remember_unlock, set_intake_start
```

The header will read **v1.1.2 | 7fd417c**. If it reads anything else the app is
stale, which is the entire point of putting it there.

| | |
|---|---|
| tag | `v1.1.2`, pushed |
| `origin/main` | `7fd417c`, equal to local |
| MSI / NSIS / APK | staged in `sustena-installers` |

## The release refused once, and it was right to

The first attempt built everything and then refused at its commit step: a
**stale** `.git/index.lock` from 14:09, with no git process alive to own it. It
left the version bump uncommitted rather than tagging something it could not
record.

I could have finished it by hand. I did not — the whole point of
`release.ps1` is that a version is never hand-edited. I reverted the bump,
committed the real work, and re-ran the one command. The artefacts and the tag
now describe the same tree, which the first attempt's would not have: it had
swept my then-uncommitted liveness work into the Android bundle while the
desktop binary predated it.

## The phone

Still off USB and off the LAN. The install is armed against the exact path the
release wrote at 14:41, so when it returns it gets **v1.1.2**, not the 03:36
build it is on now. Nothing else is waiting on me.

---

# 31 Aug, 15:10 — house in order; the phone step, ready to fire

## Stale versions: gone

22 artefacts, 2.78 GB. Exactly one version of each now exists on the machine:

```
sustena-installers/
  Mycelium-Sustena-1.1.2-x64.msi
  Mycelium-Sustena-1.1.2-x64-setup.exe
  Mycelium-Orchie-1.1.2-arm64-debug.apk
```

Removed: every 0.3.1 / 0.3.2 / 1.0.0 / 1.1.0 / 1.1.1 installer, the
`Orchie-realtime-classify-TEST` APK, three old `builds/Sustena-Android-dev-*`
APKs, the stale `target/debug` binaries, and a stray `two_nodes.exe` that had
been sitting **inside his install folder** — a dev harness has no business in
the directory his shortcut opens.

## The desktop, by hash

```
SOURCE-BUILT  794f0231a099b25d…  target/release/mycelium.exe
DEPLOYED      794f0231a099b25d…  %LOCALAPPDATA%\Mycelium — Sustena\mycelium.exe
EQUAL: True     both 16,228,864 bytes, 14:45:03
```

Header renders **`v1.1.2 · 7fd417c`**. The one commit between that stamp and
HEAD is docs-only.

## Why "grep the deployed bundle" cannot be the proof

Tauri v2 embeds the frontend **inside** the exe, compressed — his install folder
has no `dist/` at all. The control that settles it: `"no server, no network"`,
text he is reading on his own lock screen, **does not appear in the binary
either**. A string-grep there returns "absent" for a correct build, so it can
neither confirm nor refute a deploy. Chain of custody is what is available: the
embedded bundle `index-0Fo14grF.js` contains `stay unlocked`, `read from here`
and `constellation`, and the exe carrying it hash-matches what was built.

## A candidate for the staleness, offered as a candidate

`%LOCALAPPDATA%\online.vyybandasky.sustena.mycelium\EBWebView` — a WebView2
cache last written 12:49, which survives an exe swap and can serve the old UI
out of a new binary. Cleared `Cache`, `Code Cache`, `GPUCache` only; his
household lives in **Roaming** and was not touched.

His reports also predate the 14:45 copy, so "had not relaunched yet" fits the
evidence equally well. I cannot separate the two from here and am not pretending
otherwise.

## The phone

Deliberately disconnected. The armed installer waited 90 minutes and stood down
rather than spin. Nothing is lost — the APK is built and staged. When it is back
on USB, one line does it:

```powershell
& "$env:USERPROFILE\Android\sdk\platform-tools\adb.exe" install -r `
  "C:\Users\DELL\dev\sustena-installers\Mycelium-Orchie-1.1.2-arm64-debug.apk"
```

`-r` keeps his data: identity, household, peer book. The push-bug investigation
resumes when the device is back; it needs the live phone and nothing else is
blocked on it.
