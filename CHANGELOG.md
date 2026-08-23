# Changelog

Every released version of Sustena, newest first. Versions are `MAJOR.MINOR.PATCH`
and the single source of truth is the `VERSION` file at the repo root — running
`scripts/release.ps1` is the only thing that should change it.

While the version starts with `0.`, the shape of things is still allowed to
move: a **minor** bump (0.1 → 0.2) is where new capability lands and where
something may change how it behaves, and a **patch** bump (0.2.0 → 0.2.1) is a
fix that does not add anything.

## v0.3.1 - 2026-08-23

Fixes the stall after unlock: Orchie sat on its loading skeleton for ever instead of showing your household.
Also stops the loading animation cooking the phone, and makes any future stall say what went wrong instead of shimmering in silence.

- fix(orchie): never ask the engine for the Sustain called ""
- fix(devops): stage the installer for the version being released

## v0.3.0 - 2026-08-21

Orchie is a phone app now, not the cockpit shrunk to fit.
Bigger everything you tap, the keyboard finally gets out of the way, taps answer instantly, and the classifying job is the first thing on the screen.

- feat(orchie): a phone app, not a cockpit shrunk to fit
- fix(devops): read the changelog as UTF-8, and repair what was mangled
- fix(devops): write VERSION without a byte-order mark

## v0.2.0 - 2026-08-21

Working channels, one-command releases, and CI that finally covers the live Rust build.
Peer transport with quorum-gated writes and encrypted sessions; packages over the wire; a trust reading with real signals behind it; Orchie on Android; the panel-scroll and Android-launch fixes.

- fix(devops): keep the tool's output out of the exit code
- fix(devops): exit codes decide, not stderr text
- chore(devops): working channels, one version, one release command
- fix(mycelium): the debug APK aborted before its first frame; gate the bindings export to desktop
- fix(mycelium): a panel taller than the view scrolls, as a layout primitive
- feat(android): Orchie on a phone — debug APK, engine embedded
- feat(trust,orders): a reading with something behind it, and juul orders
- feat(arena): packages over the transport, with no wire-specific install
- feat(wire): authenticated, encrypted, forward-secret sessions
- chore(repo): untrack a stray SQLite journal
- feat(quorum): agreement before the append — 1000, not 700

## v0.1.0 - 2026-08-19

_Tagged retroactively. This version's installers were built and used before the
release process existed; the tag was placed afterwards on the commit that
produced them, so the history is honest about what shipped when._

The first Sustena you can install. Phase B's Rust engine (`sustena-core`) and
the Mycelium cockpit, packaged as real Windows installers with the engine
compiled in — no server, no runtime to install alongside it.

- **The engine.** `state = fold(events)`, the admission gate, typed predicates,
  composition and roll-up, the simulator, the curated feed, and the operative
  layer — ported slice by slice from the Python reference and measured against
  it with shared conformance vectors, with every divergence recorded rather
  than smoothed over.
- **Mycelium**, the cockpit: Monitor, Console, Composition, Constellation,
  Define, Economy, Ingest, Library, Network, Panels, Profile, Simulate.
- **Orchie**, the phone-first curated face, chosen by the device rather than
  the build (`pointer: coarse`), with the attention budget and the
  "why am I seeing this?" disclosure.
- **Peer transport**: two nodes converging over an authenticated socket,
  ed25519 identity, CRDT + vector-clock merge.
- **The arena**: packages that are typechecked and provenance-stamped, facing
  the same admission gate a hand-written artifact does.
- **Packaging**: Windows MSI + NSIS, branded, engine embedded.

## v0.0.x - before

Phase A: the Python reference implementation (`apps/api`, `apps/web`), which
remains in the repository, frozen, as the correctness oracle the Rust port is
measured against. It is not part of these releases and is not built by them.
