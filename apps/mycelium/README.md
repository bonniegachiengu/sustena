# Mycelium — the Sustena cockpit

**Status: V1.1 — persistent, multi-Sustain.** The household lives in an
append-only event log on disk and is rebuilt by folding it on every launch.
Still one screen; the rest of the cockpit is later slices.

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

## What is real, and what is not

**Real:** the Sustain's definition (`Σ`), its viable region (`V`), its
operators (`T`), current state (`S`), and every gate verdict — all from the
engine, none of it mocked. A refusal is rendered **as a refusal**, with the
engine's own reason text, and the state visibly unchanged behind it.

**Real:** persistence (the log above), many Sustains with real `⊕` parent/child
links, the seeded household (Homestead + Bonnie, Cira, Epha, Mum, Kui, Frankie),
a selector that switches what the Console targets, and every gate verdict.

★★ The composition tree is **validated by the engine**, not by host bookkeeping:
`MonitorEngine::flatten_holarchy` refuses a duplicate id, an unknown parent and
a cycle, and the UI shows what it said.

**Not built yet — named rather than stubbed:**

- ★★★ **roll-up `ρ` — NOT AVAILABLE, and the UI says so.** Folding children's
  state into a parent aggregate does not exist in `sustena-core` (the Python
  engine has it; the Rust port does not). Nothing is summed. A household total
  computed in the host would be a number with no rule behind it.
- **no event-log view.** The log is on disk and readable, but the cockpit does
  not render it yet.
- **no simulation, economy, governance, council, ingest, or definition editor.**
  All exist in the engine. None are wired.
- **templates are declared in Rust** (`templates.rs`), not authored in-app.
  Authoring is the Define screen.
- **no live telemetry stream.** Every read is request/response; the IPC event
  channel is the next slice.
- **no undo, no delete.** The log is append-only and nothing removes a Sustain.
- **no Orchie.** Mobile is a later face on the same codebase.
- **desktop only.** `[lib] crate-type` is `["lib"]`; the mobile crate-types
  (`staticlib`, `cdylib`) go back when the mobile target lands.

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
