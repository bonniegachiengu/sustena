# Operator audit — 26 Aug 2026

*Are we on track for "everything is an operator, operators compose into
operative-DAGs, the operatives ARE those DAGs (Mentor = finance, Attaché =
networking)"?*

Read from the code, not from memory. Every claim below has a check beside it.

---

## Verdict

**The gate half is on track. The composition half does not exist yet.**

Every capability that changes a household's money is a registered operator
behind the admission gate, and that holds without exception. Nothing composes
those operators into a graph, and nothing names a graph as an operative.

So we have the primitive and not the thing it was a primitive *for*.

---

## On track

### Every state change is an operator

18 registered operators, `test.*` excluded:

```
budget.add_pocket      budget.allocate         budget.open_account
budget.place_unaccounted  budget.reclassify    budget.record_income
budget.spend           budget.transfer         budget.unallocate
budget.unrecord_income budget.unspend          device.heartbeat
inventory.consume      inventory.itemize       roster.admit
vendor.identify        vendor.remember         vendor.suggest
```

**Checked by the inverse**, which is the check that matters: `transducer.rs`,
`parse_rule.rs`, `parse_rule_learn.rs`, `effect_capture.rs`, `enzyme.rs`,
`curated.rs`, `ingest.rs` and `orchie.rs` contain **zero** calls to
`state.set/append/increment/decrement` between them. The deciding logic cannot
move money even by accident, because it has no way to write.

Three write sites exist in the app and each is accounted for:

| Site | What it is |
|---|---|
| `World::call` | the operator path — gate, fold, events |
| `World::reload` | replays the log; a rebuild, not a bypass |
| `World::transfer` | cross-sustain atomic move via `holon.rs`, which a single-sustain operator cannot express |

### Read-only operators are established

`roster.admit`, `vendor.identify` and `vendor.suggest` declare no side effects
and zero pawa. This is the shape a DAG node needs for a step that decides
rather than changes, and it now has three working instances. The pattern is
proven; nothing consumes it yet.

---

## Deviated

### 1. There is no operative-DAG layer

`operative.rs` is a **different thing entirely** — the game-theoretic operative
from the Operative article (utility vector ω, Ω ranking, Cynefin domains). It
is not a graph of operator calls.

- `OperativeGraph`, a node/edge runner, or anything that sequences operator
  calls at runtime: **absent**.
- Mentor and Attaché: **absent**. Those names appear only as test fixture
  strings in `agent.rs` and `cynefin.rs`.

So any operative-shaped behaviour today is a hardcoded function on a screen.

### 2. A composition layer exists and has never been called

`compose.rs` implements Hoare sequencing over operator steps —
`post(A) ⊨ guard(B)` — and produces a statically checked `Pathway`.
`Pathway::try_chain` has **no callers outside its own tests**.
`semantic.rs::replay_under` runs a linear call sequence through registry and
gate, and is reached only from `enzyme.rs` tests.

The machinery for *linear* composition is built and unwired. This is the same
state `inverse::invert` and `preattentive::encode_field` were in before they
were wired; those two were wired by finding a real caller, and this needs the
same.

### 3. The deciding logic is bespoke

Parsing, rule learning, classification inference, transfer detection, reversal
netting, skip learning, reclaim, curated `compose(r)` — none is registered, so
none can be a node in a graph. They are correct and well-tested; they are just
outside the architecture.

### 4. Household knowledge lives outside the fold

Seven side stores sit next to the event log:

| File | Holds | Should it fold? |
|---|---|---|
| `history.json` | vendor → pocket memory | **yes — and it is now duplicated** |
| `skip_rules.json` | "never ask me about these again" | yes: a decision he made |
| `rules.jsonl` | parse rules he corrected | yes: learned household knowledge |
| `own_identifiers.json` | his own phone / account numbers | no: device config, and deliberately not synced |
| `messages.jsonl` | the capture log | no: it is its own append-only log |
| `read_marks.json`, `sources.json` | device bookkeeping | no |

**The duplication is live and is mine.** `vendor.remember` writes a `vendors`
state dimension while `ingest.rs::remember` still writes `history.json`, and
the classify UI still reads the side file. Two sources of truth for the same
question, and the wrong one is currently authoritative. Nothing is corrupt —
they simply disagree about who knows what — but it must be resolved before
either is trusted.

### 5. `egress` is not ported

Present in Rust only as a capability string in a `learned.rs` test.

---

## Realignment

In dependency order. Each step is small and none needs the next to be useful.

1. **Resolve the vendor duplication.** One source of truth: the `vendors`
   dimension. Point the classify UI at `vendor.suggest`, migrate what
   `history.json` holds through `vendor.remember`, retire the side file.
2. **Read-only operator wrappers for the decision functions** — `parse.classify`,
   `capture.infer`, `transfer.detect`, `reversal.match`. They take state and
   params, return a verdict, change nothing. This is what makes them graph nodes.
3. **Fold the learned knowledge.** Skip rules and corrected parse rules become
   operators writing state, so a rebuild reproduces what the household has
   learned.
4. **Port the DAG runner** — nodes are operator calls, edges carry conditions,
   `$dot.path` injects a prior node's output.
5. **Declare Mentor and Attaché as specs** over those operators. Mentor's
   finance DAG is already visible in what exists:

   ```
   parse.classify ─► vendor.identify ─► vendor.suggest ─► budget.spend ─► vendor.remember
                                                     └──► (nothing known: ask)
   ```
6. **Wire `compose.rs`** so each linear segment of a declared DAG is checked
   before it is allowed to run.

Steps 1 and 2 are worth doing regardless of how the money model settles. Steps
4 to 6 should wait for it, because the DAG's shape depends on what the model
turns out to need.
