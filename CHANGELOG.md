# Changelog

Every released version of Sustena, newest first. Versions are `MAJOR.MINOR.PATCH`
and the single source of truth is the `VERSION` file at the repo root — running
`scripts/release.ps1` is the only thing that should change it.

While the version starts with `0.`, the shape of things is still allowed to
move: a **minor** bump (0.1 → 0.2) is where new capability lands and where
something may change how it behaves, and a **patch** bump (0.2.0 → 0.2.1) is a
fix that does not add anything.

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
