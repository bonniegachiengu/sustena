# Orchie as Ingest, Monitor and Controller

*The work we can do on Orchie, in order, with each step traced to the article that specifies it.*

**Status: P1 shipped and installed. P2, P3, P4 open.**

> Read this alongside `R2_BACKLOG.md`. The same rule applies here: the articles
> are the spec and the code catches up to them. Where an article and the code
> disagree, level up to the better design rather than editing the doc to match
> what was built.

Articles referenced, all in `Articles (Serious)/technical/`:

| Short name | File |
|---|---|
| Ingest | `3B1B — Ingest - Formal Execution.md` |
| Monitor | `3B1B — Monitor - Formal Execution.md` |
| Controller | `3B1B — Controller - Formal Execution.md` |
| Curated UI | `3B1B — Curated UI - Formal Execution.md` |

---

## Why these four steps, in this order

Orchie is the phone, and the phone is three things at once. It **senses**, because
M-Pesa and KCB texts arrive there. It **watches**, because it is the thing that
would notice if sensing stopped. And it **asks**, because every write still waits
for a person.

The order below is a dependency chain rather than a preference. P2 gives the
system something to watch. P3 turns what it watches into a series over time. P4
draws that series so a glance carries it. Each one needs the one before it.

---

## P1. On-device capture (DONE)

**Article:** Ingest §I (the boundary as a sensor), §VI (one write path).

**Shipped 2026-08-25.** A Tauri plugin crate, `crates/tauri-plugin-sms-capture`,
reads M-Pesa and KCB texts on the phone. A manifest-registered receiver captures
them while the app is closed, two filters run on the device, and a local queue
holds them until the app is open and unlocked.

§VI is the part worth restating, because it is the reason the design looks the way
it does. There is no ingest write path. Captured text takes the same door as a
pasted message, `world.capture`, which means the same transducer, the same rules
and the same admission check. The receiver never opens a socket and never touches
the log, which is exactly what lets it work while the phone is locked: capturing
needs nobody, and applying needs the key.

Verified on the device: the Gradle module registers, the manifest merges
`READ_SMS` and `RECEIVE_SMS`, the receiver is live at priority 999, and the full
chain from the webview through Rust to the Kotlin class returns a real answer.

**Untested:** no real text has been through it. The permission is Bonnie's to
grant.

---

## P2. The phone as a Sustain, and heartbeats

**Articles:** Ingest §IX (device-as-Sustain), Ingest §VIII (liveness and
heartbeats), Monitor §VIII (belief state under silence).

### What the articles ask for

Ingest §IX says a sensor is a Sustain, with no bespoke alerting subsystem
anywhere:

```
S_dev = (connectivity, battery, app_version, queue_depth, t_last_ack)
V_dev = { battery >= b_min AND queue_depth <= Q_max AND now - t_last_ack <= theta(c) }
```

Everything then follows from machinery that already exists. Staleness becomes an
ordinary invariant under the existing gate. A flat battery escalates by the same
path as an overdrawn pocket. The connection panel is a view over `S_dev`.

§IX calls out `queue_depth` as the leading health metric, because it rises before
anything else visibly breaks: a device that cannot deliver keeps accepting. A
health model built only on last-contact lags by construction.

One structural point from §IX shapes the whole slice: **a liveness invariant
cannot be self-enforced.** A dead phone cannot write its own `t_last_ack`, so that
clause has to be evaluated by the receiving Sustain across the composition. This
is the roll-up reading a child's dimension, and the same reason a whole folds a
part's state rather than trusting the part's self-report.

Ingest §VIII explains why heartbeats matter more than thresholds. Percentile
staleness declares a false-alarm rate rather than discovering one, and on a bursty
signal like household money a confident alarm is necessarily a late one. A
heartbeat every `h` removes the statistics: staleness stays under `h + delta`
deterministically, so the threshold becomes a decision instead of an estimate.

Monitor §VIII covers what happens during silence. Certainty about a silent device
decays rather than freezing, and the escalation is driven by growing uncertainty.

### What exists in Rust today

- `belief.rs` has `Belief`, `BeliefTracker` and `BeliefReading`, with conformance
  vectors in `conformance/vectors/belief.json`.
- `monitor.rs` has `SustainWatch::believing(SilenceSpec)`, so silence is already a
  declarable property of a watch.
- Composition and roll-up are built and in daily use, which is what §IX needs for
  the cross-composition evaluation.

### The gap

Nothing models the phone as a Sustain. `heartbeat`, `last_ack`, `queue_depth` and
`connectivity` return zero hits across `sustena-core` and the host.

The app's one use of `MonitorEngine` is a structural check on the shape of the
tree, and it passes an empty `Region` with a nominal detector. The code says so in
a comment: real thresholds belong to the Monitor screen, and inventing them there
to make a structural check compile would be declaring rules nobody chose. That
comment is right, and it also means no real region is being watched.

So today Orchie goes blind silently. If capture stops, nothing says so.

### First slice

Declare a device Sustain and give it two real dimensions before adding any more.

1. A `device` template with `queue_depth` and `t_last_ack`, linked under the
   household as a child.
2. The drain writes both on every sweep. `queue_depth` comes from the queue it
   just read, `t_last_ack` from the sweep itself.
3. The household carries the liveness invariant, evaluated by the parent rather
   than by the phone, per §IX.
4. One card in Orchie reading that child's state.

Battery, connectivity and `app_version` come after, because they need platform
calls and the first two do not. Heartbeats come after that, since a heartbeat is
only meaningful once something is checking for its absence.

---

## P3. Give urgency a history

**Articles:** Monitor §VI (CUSUM), Monitor §V (EWMA), Monitor's ratified addition
of 2026-08-05 (urgency is distance-to-V), Curated UI §IV (salience).

### What is already true, and worth correcting

The plan called for replacing the `spent / allocated` proxy with real
distance-to-V. **That replacement already happened, in both the core and the app**,
and this tracker should say so.

`curated.rs::urgency_of` reads `region.distance(state)` and nothing else. Its own
comment records why: a ratio has a denominator that can be zero, which is what made
an unfunded pocket that had been spent from score as calm. There is no denominator
now.

`orchie.rs::region_from` builds the region out of the household's own numbers. A
floor at zero on liquid, a ceiling at whatever the household allocated to the pocket
under most strain, and a band of exactly zero on unclassified captures. Reading a
household's own allocation is using their number rather than declaring a threshold,
which is what kept this inside V1.2's refusal to invent a region.

So the ratified position holds today: urgency is the distance to V, there is one
notion of important, and Curated UI §IV consumes it as salience.

### The gap

The distance is computed fresh on every `compose(r)` call and then forgotten. There
is no series, so nothing can be smoothed and no drift can be detected.

That costs the two things Monitor §V and §VI exist to provide. Nothing smooths the
signal, so a figure that jitters between reads trains the eye to ignore it, which is
§V's opening argument. And nothing detects a persistent drift, so a household
spending slightly over allocation every day for two weeks looks identical to one
that had a single bad afternoon and recovered. §VI names that case directly: a small
persistent shift is the more alarming one, and CUSUM is the detector built for it.

The chain itself is finished in the core. `R2_BACKLOG.md` rows 23 and 24 record it:
`region.rs` made urgency the real distance, `detect.rs` added EWMA and CUSUM over that
series with the severity classification that is the Monitor to Controller boundary, and
`monitor.rs::ingest()` runs the three as one pass per sustain.

What is missing is a caller. The app builds its watch with an empty region, so the
engine that would hold the series is watching nothing.

### First slice

Give the household's own distance a history.

1. Build the `SustainWatch` for the household from the real `region_from`, rather
   than the empty `Region` used for the structural check.
2. Feed each computed distance into `MonitorEngine::ingest`, so EWMA smoothing and
   CUSUM both see the series.
3. Read urgency for salience off the smoothed value.
4. Surface a CUSUM crossing as its own attention row, worded as a persistent drift
   rather than a threshold breach, because that is what it detects.

The escalation boundary in §VI is where Monitor hands to Controller. Worth knowing
before the slice: Monitor's own note frames that crossing as an economic decision,
since surfacing something spends one of the four attention slots the Curated UI
budgets.

---

## P4. Drawing salience

**Articles:** Monitor §VII (preattentive processing, `WidgetVisualEncoder`),
Curated UI §IV.

### What the article asks for

§VII rests on Treisman's Feature Integration Theory: the visual system processes
some attributes in parallel across the whole field before attention engages, in
roughly 150 to 200ms. The article maps them:

| Attribute | Use |
|---|---|
| Hue | constraint health, green to amber to red |
| Brightness | urgency intensity |
| Size | magnitude, for example ring radius |
| Motion | active computation |
| Orientation | trend arrows |
| Enclosure | widget boundary as sustain boundary |

The goal it states is discriminability: the chance a person reads the system's state
correctly from a glance.

### What exists in Rust today

`preattentive.rs` has the whole vocabulary. `VisualAttribute`, `Channel`,
`DataDimension`, `Hue`, `Motion`, `Arrow`, `Fraction` and `VisualSpec`, with
conformance vectors in `conformance/vectors/preattentive.json`, plus `widget.json`
alongside it.

The entry point is `encode_field(field: &[&Ingested]) -> Vec<VisualSpec>`.

### The gap

`VisualSpec` never reaches a screen. The host does not call `encode_field`, and the
frontend contains no hue, brightness or motion mapping. Cards render in one weight
whatever their score.

This is the last mile the Curated UI note describes: salience is computed correctly
and then drawn flat, so the ranking exists in the data and never reaches the eye.

The signature is also the sequencing constraint. `encode_field` takes `Ingested`,
which is what `MonitorEngine::ingest` returns, so **P4 needs P3 first.** Without a
watch producing readings there is nothing to encode.

### First slice

One attribute, end to end, before any others.

1. Call `encode_field` on the readings P3 produces.
2. Carry one attribute through the DTO, hue for constraint health, since it is the
   one with a settled three-way meaning already in the palette.
3. Render it on the Orchie card border.
4. Check it at a glance on the phone rather than in a test, because discriminability
   is the claim and a test cannot make it.

Brightness for urgency is the natural second, and it is the one that makes the
knapsack's ranking visible. Motion comes last and should stay rare, since a card
that pulses without cause is the flicker §V warns about.

---

## What this chain adds up to

P1 gave Orchie a sense. P2 lets it notice when that sense fails. P3 gives it memory
of what it sensed. P4 lets a person take all of it in without reading.

The Controller article is the frame the four sit inside. §IV puts the human at the
decide step, which is already true in Orchie: money in files itself, money out waits
for a pocket. §VIII's persistent panel invariant is the reason the attention budget
stays fixed while the contents change. Neither needs new work for P2 to P4, and both
are worth rereading before the escalation wording in P3, since the Monitor to
Controller boundary is where they meet.

---

*Last updated: 2026-08-25. P1 installed on the phone. P2 is the next slice, and the
device Sustain is the piece to build first, because everything after it needs
something real to watch.*
