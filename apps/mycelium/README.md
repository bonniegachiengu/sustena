# Mycelium — the Sustena cockpit

**Status: v1 complete (V1.6), and the honest absences are being closed —
roll-up `ρ` is now real.** Twelve panels, built from one shared component
system. Nine are wired to real engine capability; three describe a subsystem
`sustena-core` does not have, in the cockpit's own layout, naming what exists
and where the capability lives. The look holds from a wide desktop down to a
375px phone with no horizontal scroll, from responsive *primitives* rather than
per-screen media queries.

```bash
npm install
npm run tauri dev          # builds the Rust host, generates bindings, opens the window
```

## What this is

```
  SolidJS + TS (strict)            src/
        │  typed IPC, generated
        ▼
  bindings.ts   ◄── GENERATED ───  src-tauri/src/dto.rs   (specta)
        │
        ▼
  Tauri v2 host                    src-tauri/src/{commands,engine}.rs
        │  a direct function call
        ▼
  sustena-core                     ../../sustena-core     (compiled IN)
```

There is **no server and no network**. `sustena-core` is linked into the
binary, so a command is a function call and the app works with the machine
offline.

## The typed boundary

`src/bindings.ts` is **generated** from the Rust in `src-tauri/src/dto.rs`.
Never edit it. Regenerate either way:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin export_bindings
# ...or just run the app: a debug start rewrites it before the window opens.
```

Change a field in `dto.rs` and `npm run build` (which runs `tsc --noEmit`)
fails until the UI agrees. That is the point.

### Why the DTOs are here and not in `sustena-core`

ADR-0001 fixes the engine's dependencies at `serde` / `serde_json` /
`thiserror` so it compiles unchanged to WASM, Android, iOS and desktop. Adding
a codegen crate to the engine to suit a desktop UI would be the host's problem
leaking into the portable core. The core also has no I/O and no opinion about a
wire format — which is exactly what a DTO is. See the header of `dto.rs`.

## Persistence — the log, not a snapshot

```
%APPDATA%/online.vyybandasky.sustena.mycelium/
  sustains.json                the registry — id, label, template, parent, selection
  events/<sustain_id>.jsonl    one event per line, append-only
```

The engine's central property is `state = fold(events)`. Persisting a **state
snapshot** would make the file the source of truth and the log a story about it
— backwards, and unresolvable the first time they disagreed. So what survives a
quit is the **log**, and `Store::load_state` folds it with the engine's own
`fold_events`. That is the only way to obtain state, so the property cannot
quietly stop being true.

Every append is followed by `sync_all()`. The registry is written to a temp file
and renamed, so a crash mid-write leaves the previous one intact.

**Honest limits of the format:** no compaction (a long-lived Sustain's log only
grows), no concurrent writers (one app instance), and a torn final line after a
hard power loss would need dropping by hand — the loader names the file and the
line rather than failing vaguely.

Prove it without the GUI:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin smoke
```

It opens a household, runs real operators, **drops the World**, reopens from
disk alone, and asserts `cached state == fold(persisted log)` for every Sustain.

## The push channel — the event IS the message

```
committed operator call
        │  (a refusal emits NOTHING)
        ▼
  Committed { sustainId, operator, seq, events, mutations, state, constraints, liquid }
        │  tauri-specta typed event
        ▼
  src/lib/live.ts   ← ONE subscription for the whole app
        │  createStore + produce/reconcile — merges by path
        ▼
  every screen reads reactively; only the cells that moved repaint
```

**There is no tick and no sampler.** The core has no clock, and sampling would
manufacture events nothing caused — so the host emits exactly once per
committed change, carrying the new state, the events it published and `V`
re-evaluated. A subscriber is correct after a single message without replaying
anything.

**A refusal pushes nothing**, because nothing changed. The verdict is the
Console's business; the world's state is the channel's.

**No polling anywhere.** `refreshWorld`/`hydrate` run on mount and when *you*
change the selection — never in response to a change. `Console.execute`
deliberately does not re-fetch after a commit: asking again would hide a broken
channel behind a re-read.

**Honest cost, named:** the message carries the whole state of the changed
Sustain. Fine for a household; a very large Sustain would want a delta, and the
mutations are already in the log if that day comes.

## One selection model

The topbar selector, the left nav and the Constellation all read and write
`world.selected`. Clicking a node selects that Sustain **and** opens Monitor on
it. Nothing can disagree about what you are looking at.

## The economy is real, and bounded

Every committed call now runs through `execute_afforded` with
`Affordability::Metered`, so **`balance ≥ pawa` is a clause of the real gate**
— priced against the candidate's actual cost, which the gate's effect-on-a-copy
makes free. The `serving` seam is opted in, so this host earns
`rate × pawa_served` for the work it did, at the **governed** rate.

- `pawa::meter` produces every reading; a `PawaReading` has no public
  constructor, so a debit cannot be invented.
- `JuulLedger::charge` takes that reading, **not a number**.
- `Parameters` are read from a governance Sustain's state — never a constant.
  `governance.set_parameter` is the only way to change one, and its bounds are
  ordinary invariants.

★★★ **Internal points only.** Juul is an accounting unit on this machine —
never real money, never transferable, never a payment rail (ADR-0001 D5). The
notice is carried from the host as data so a surface cannot forget to show it.

★ **Genesis is declared, not calibrated** — `1_000_000` juul to the local
principal, chosen so a household never trips the affordability clause by
accident, in exactly the way `κ` is a declared number.

## Simulate — a fork, not a simulator

STEP-0 found **no scenario-fork API in `sustena-core`** (`ensemble::Scenario` is
model ensembles, not Sustain simulation). So a branch is exactly `state.clone()`
plus the **same `execute_admitted`** a real call uses. It is the engine's gate on
a copy — a step refused in a branch is refused for real, with the same reason.

Nothing is written: no log, no ledger, no meter, no cached state. Promote
replays through the real `run_operator` and **does not trust the simulation** —
a step can honestly refuse if live state moved.

## Define — the engine decides what lands

`editing::typecheck` parses every invariant and **binds it against the schema**;
`editing::safe` checks the candidate against every live instance. A definition
that fails either is **never persisted**, so loading one can never surface a
rule that was broken when it was authored.

```
well-typed        -> Accepted
undeclared dim    -> NotWellTyped ["rainfall: invariant 'rain_ok': undeclared dimension"]
persisted         -> ["garden"]        (only the well-typed one landed)
edit that strands -> WouldStrand ["garden-1 · soil_rich (soil (40.0) >= 90 (90) failed)"]
```

An authored definition is instantiated, gated and logged through **exactly** the
path a built-in uses.

★ **A finding, reported rather than smoothed over:** an unparseable expression
(`soil >>= ??`) comes back as *"undeclared dimension >="* rather than a syntax
error — the engine's predicate parser partially consumes it. Still a refusal,
still nothing written, but the message misleads. The UI shows the engine's words
verbatim rather than inventing a friendlier lie.

## Profile — the real capability model, honestly unenforced

`effective_privilege` walks the membership path with the **weakest-link** rule;
`permitted` compares it against each operator's declared `min_privilege`.

★★★ **And the gate is not checking any of it.** Every call runs
`Authorization::Unchecked`. The screen shows the authority the principal
*holds*, not one being enforced — a permission matrix implying enforcement would
be security theatre.

## What is real, and what is not

**Real:** the Sustain's definition (`Σ`), its viable region (`V`), its
operators (`T`), current state (`S`), and every gate verdict — all from the
engine, none of it mocked. A refusal is rendered **as a refusal**, with the
engine's own reason text, and the state visibly unchanged behind it.

**Real:** persistence (the log above), the push channel, many Sustains with real
`⊕` parent/child links, the seeded household (Homestead + Bonnie, Cira, Epha,
Mum, Kui, Frankie), a selector that switches what both screens target, every
gate verdict, the **event-log view** (rendered from disk — the log V1.1
persisted, now visible), and `V` evaluated live.

★★ **`V` is evaluated by the engine, not by the app.** Each invariant goes
through `sustena_core::predicate::check` — the same evaluator the gate uses —
and the UI shows its verdict and its reason text. A rule whose expression will
not parse is reported as **not holding**, because treating an unparseable rule
as satisfied is the one failure mode a viable region must not have.

★ **One threshold IS this app's, and is labelled so:** the ≥80% pocket
attention line. The engine's real urgency notion is `d(s,V)` over a declared
`Region`, and these templates declare invariants rather than intervals —
inventing a Region to borrow its authority would be declaring thresholds nobody
chose. So it is a stated policy over real numbers, shown as ours on the card.

★★ The composition tree is **validated by the engine**, not by host bookkeeping:
`MonitorEngine::flatten_holarchy` refuses a duplicate id, an unknown parent and
a cycle, and the UI shows what it said.

**Not built — named rather than stubbed:**

- ★★★ **`holon.transfer` — NOT AVAILABLE, and deliberately not approximated.**
  An atomic conserved move between two Sustains is in the Python engine and not
  in `sustena-core`. The obvious host workaround — spend here, record income
  there — is two separate gated calls: if the second refuses, money has left one
  household and arrived nowhere. That is not a transfer, it is a way to lose
  money that looks like a feature. (`juul::transfer` exists, but that moves the
  *economy's internal unit* between principals, not a household's money between
  Sustains.)
- ★★ **ingest / the transducer.** No parser turns an SMS or a bank alert into a
  proposed call. Not written here on purpose: the Python transducer's own
  history is speculative patterns that matched no real message until real
  samples arrived, and a wrong parse of a financial message is a wrong ledger
  entry.
- ★★ **peer transport.** The distributed primitives are real and tested in the
  core (CRDTs with convergence laws, vector clocks, Paxos-style consensus, the
  router). There is no socket, no discovery and no gossip — so this is a single
  node, and the panel says `— single node` rather than `0 peers`, which would
  imply a network that found nobody.
- ★★ **the arena / library.** No package registry, and it depends on the
  transport that does not exist.
- **the council's proposal lifecycle.** `council::resolve` is real and wired;
  persisted proposals, deadlines and operatives that actually deliberate are
  not — the votes are yours to set, so it is the engine's rule engine exercised
  by hand.
- **authority is displayed, not enforced.** `effective_privilege` and
  `permitted` are the engine's, but every call still runs
  `Authorization::Unchecked`. The identity `bg.myc` is declared, not
  authenticated.
- **the topbar clock is the UI's**, not the engine's. The core has no clock and
  nothing on screen attributes a timestamp to it.
- **no undo, no delete.** The log is append-only and nothing removes a Sustain.
- **no log compaction.** A long-lived Sustain's log only grows.
- **no Orchie.** The phone face is a later product on this same codebase — the
  responsive primitives above are its groundwork.
- **desktop only.** `[lib] crate-type` is `["lib"]`; the mobile crate-types
  (`staticlib`, `cdylib`) go back when the mobile target lands.

## Roll-up ρ — real, and it refuses to fabricate

`sustena_core::rollup` folds a Sustain's **own** state and every linked child's
into the aggregates its `Σ` declares. The homestead declares two:

```
household_liquid_total    sum over finances.liquid.balance
household_pockets_total   sum over finances.pockets[*].allocated
```

Three properties, each a refusal rather than a feature:

- **Fresh every call, never persisted.** There is no stored total anywhere, so a
  figure here can never be one that quietly stopped being true. Call it twice
  against unchanged states and it reproduces itself.
- **An unreadable member is EXCLUDED AND NAMED, never counted as zero.** A
  silent drop is arithmetically indistinguishable from a member who genuinely
  holds nothing. `included`/`excluded` travel with every value, the exclusion
  carries the engine's own reason, and the UI shows both or neither.
- **The household's own contribution is in.** A "household total" that skipped
  the household would answer *what your members collectively hold*, which is a
  different question from the one the label asks. The reference shipped it
  children-only and corrected it for exactly that reason.

★★ **The host does not sum anything.** It reads states and hands them to
`compute_rollup`, the same division of labour as `V`, where it asks
`predicate::check` about a rule rather than judging one.

★ `AggregateReading` has **no public constructor**: the only way to hold one is
to have computed it, so a total cannot travel without the lists that say what it
covered — the same structural guarantee `PawaReading` gives a debit.

Prove it without the GUI:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin smoke
```

The last section computes ρ over the real seeded household and checks it against
a **hand sum taken from the same states**, then adds one unreadable member and
confirms it is named and that the total **does not move**.

## The push channel carries three events

| | `Committed` | `Refused` | `RolledUp` |
|---|---|---|---|
| answers | *what changed* | *what was declined* | *what a household now totals* |
| carries state | **yes** | **no field at all** | no — a derived reading |
| about | the Sustain that changed | the Sustain that asked | its **parent** |

★★ ρ is its own event because it is a **different Sustain's** figure. A member's
commit moves its household's total, and folding that into `Committed` would mean
a message about Bonnie's habitat carrying the homestead's numbers under a field
name that did not say so. A refusal emits none of the three's state: ρ is a
function of state, and a refused call changed none.

## The design system — `src/ui`

```
  src/ui/tokens.css.ts    colours, type scale, spacing, radii — and the breakpoints
  src/ui/layout.css.ts    arrangement: frame, Split, Column, Stack, Cluster, the nav rail
  src/ui/ui.css.ts        the instruments: card, verdict, absence, row, meter, badge, dot
  src/ui/index.tsx        the components every screen imports
```

**One definition each, consumed everywhere.** Before V1.6, `app.css.ts` held 98
ad-hoc exports grown a section per slice, and the same card / verdict /
empty-state was re-spelled on each screen. The pieces that were duplicated are
now single components:

| | why it has to be one thing |
|---|---|
| `Card` | the instrument frame; every panel is made of them |
| `Verdict` | a gate refusal that rendered differently in two places is one a person learns to read twice |
| `Unavailable` / `Absent` | the designed absence — the most load-bearing component in the product, because it is how the app tells the truth about itself |
| `Empty` / `ErrorState` | the honest-state vocabulary, in one voice |
| `Hypothetical` | amber and dashed, so a fork can never be misread as a fact |
| `Boundary` | solid, not dashed — the economy boundary is a standing fact, not a gap |
| `Row` / `Readout` / `NoteRow` / `Meter` / `TelemetryCell` | the reading families |
| `Badge` / `Dot` | status with fixed semantics: teal ok, amber warn, red danger, dashed absent |

### Responsive, as primitives

★★★ **No screen file contains a media query.** Breakpoints are tokens (`bp.md`
900px, `bp.sm` 560px) and only `src/ui` consumes them — `layout.css.ts` for how
instruments sit next to each other, `ui.css.ts` for the three places an
instrument adjusts its own density (a big figure, a card's padding, a
three-column control row). A screen composes `Split`/`Column`/`Stack`/`Cluster`
and inherits narrow-width behaviour it never had to think about.

Every primitive sets `min-width: 0`. Without it a grid or flex child refuses to
shrink below its content and pushes the page sideways — the single most common
cause of the horizontal scroll this arrangement exists to prevent. `Fill` is
that fix as a component, for the one-off spans inside a row.

The two places that *do* scroll sideways do it **inside themselves**: the nav
rail (which becomes a horizontal strip below `md`) and the status belt. The page
never does.

★★ This is also the groundwork for **Orchie**. A phone-first face on this same
codebase needs primitives that already collapse correctly, not a second set of
screens.

### One symbol rule

`text-transform: uppercase` turns `ρ` into `Ρ` — a different letter. The
cockpit's labels are full of Greek that carries meaning, so `Sym` holds a symbol
out of the transform and a title can be uppercase and still say `ρ`.

## Local notes

- The dev server binds **127.0.0.1:1421**. Port 5273 was tried first and sits
  inside a Windows reserved range (`netsh interface ipv4 show
  excludedportrange` → 5241–5340), and the default IPv6 bind fails `EACCES` on
  `::1` before reaching IPv4.
- Bindings are generated by a plain **bin**, not a test: an integration-test
  binary that links the Tauri runtime fails to load on Windows with
  `STATUS_ENTRYPOINT_NOT_FOUND`.
- `cargo run --bin smoke [dir]` runs the persistence proof headlessly. With no
  argument it uses a scratch dir under the system temp, so it never touches the
  real household; pass the app data path to inspect the real one.

## The frozen app

`apps/web` is the **previous** UI (React, on the Python engine). It is frozen
reference and is not touched by anything here.
