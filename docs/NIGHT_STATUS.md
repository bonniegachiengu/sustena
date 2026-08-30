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

(updated as it lands)
