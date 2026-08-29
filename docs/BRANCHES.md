# Branch log

*Every branch, what it is for, where it stands, and what merging it needs.*

`main` is protected and always releasable. Nothing lands on it without care, and
releases are cut from it and nowhere else. `dev` is where finished work meets
other finished work. Working branches fork from `dev` and merge back when green.

Keep this file current in the same commit that changes a branch's state. A
branch nobody can describe is a branch nobody can safely merge.

---

## Live

*(none open right now.)*

---

## Merged

### `feat/structural-secret-detector` — merged into `dev` 29 Aug — **IMM-11**

**Off:** `dev` · gate green (core 1,721 · host 245).

**Three layers checking the same ten regexes are one layer written three times.**
The Android filter, the ingest engine and the transducer each refuse a message
carrying a one-time secret, and that reads as defence in depth until you ask what
happens when the vocabulary misses: all three miss together, for the same reason.
Layering buys **independence of failure**, not a count.

★★★ **So the second detector reads no words at all.** A bare 4–8 digit code —
necessary and not sufficient, because a real paybill confirmation genuinely
contains a bare eight-digit account number — plus three of four shape features:
no transaction reference, brief, a shouted three-word run, no figure quoted in
cents. Combined with the vocabulary by **OR**, because the costs are not
symmetric: a false positive is one capture somebody re-enters, a false negative
is a live credential in a database.

★★★ **Independence proven rather than claimed, in both directions.** A credential
message in Swahili passes the ten regexes completely untouched and is refused on
shape; a wordy warning containing no code at all is invisible to shape and caught
by vocabulary. Before this branch the Swahili message parsed as ordinary text and
was stored.

★★ **Calibrated against the real corpus, and it moved the design.** The obvious
structural rule is "a credential message does not mention money" — and the real
OTP sample names a USD transaction, so that rule would have missed the one
message it was written for. It is caught on `no_cents` instead: money is quoted
to two places and `113.8` is not. All eight real transaction shapes stay in the
tests as a standing negative control, because a detector that refuses real
payments is one somebody turns off, and then there is no second layer at all.

★★ **A real bug the corpus found.** The first pass disqualified any token
containing a full stop as an amount, which read the sentence-ending period in
`000000.` as a decimal point and missed a real TAN code. Separators are only
amount-markers **between** digits.

⚠ **Disclosed, not done: the Android copy.** The device-side Java filter still
has only the vocabulary. Writing a fourth hand-synced copy of a detector would
re-create the exact coupling this row exists to break, and the right fix is for
the phone to call one shared check — a larger change than this branch. **Until
then this layer protects the store, not the wire**: a credential in unfamiliar
wording still leaves the phone before anything refuses it.

### `feat/metrics-traces` — merged into `dev` 29 Aug — **MON-3**

**Off:** `dev` · gate green (core 1,708 · host 245).

**Three pillars, and two of them were already in the third.** §III asks for
Metrics, Logs and Traces. Logs shipped long ago as the event log. Building the
other two as separate stores is the conventional shape and it would have been the
state-cache mistake a second time — a counter written *beside* the log by the same
code can disagree with it, and nothing can say which is wrong.

★★★ **Nothing here is stored.** A metric is a fold evaluated on read; a trace is
the `causes` graph the log already is. They cannot drift because there is no
second write path to be inconsistent with the first. The cost is named rather
than hidden: every read is a scan. That is right at a household's volume and
wrong at a million events a second, and when it stops being right the answer is a
cache that is provably a fold — not a second writer.

★★★ **A span has no duration.** OpenTelemetry's carries `start` and `end`; an
event carries `t_event` and nothing about how long anything took. Deriving one —
"it ended when the next event began" — would be a fabricated measurement, and a
fabricated measurement is worse than an absent one because it looks like
evidence. `elapsed_ms` is the reading that genuinely exists: how long the world
took to finish reacting to a cause, which is explicitly not CPU time.

★★★ **`TraceCollector` declined.** `causes` is written when the events are, so a
collector would be a second recording of a fact the log already holds — the same
objection as the metrics store, one layer up.

★★ **Multiple causes are kept.** An event can have several, and picking the first
to force a tree invents a causality nobody recorded. **An orphan is reported, not
reparented**: a window boundary is not an origin, and making one a root turns "we
did not fetch far enough back" into "this is where it started". Traversal is
bounded by span count, so a malformed window terminates and reports instead of
overflowing.

★★ **`LabelOf` is an enum, and that is the cardinality defence.** The unbounded
label values here would be payload text — a merchant name, a typed description.
Naming the legal sources exhaustively means a free-text label cannot be declared
by mistake. `cardinality()` is askable because a cardinality failure is never
caught by a series being *wrong*; every one is individually correct, and there
are fifty thousand of them.

### `feat/egress-asymmetry` — merged into `dev` 29 Aug — **ING-11**

**Off:** `dev` · gate green (core 1,693 · host 245).

**Egress is not ingest run backwards.** `outbox.rs` retries on `Unknown` and is
right to — the destination is this engine, capture is keyed by fact, a duplicate
is swallowed. Egress cannot borrow that: **we cannot make someone else's receiver
idempotent.** There is no key to send, nobody to ask to honour it, and a second
send of the same payment is a second payment.

★★★ **The same `Unknown` produces opposite correct answers on the two sides**,
and the test asserts both in one place rather than describing the difference in a
comment. It is the whole row: identical input, opposite disposition, and the only
thing that changed is who owns the receiver.

★★★ **`Reversibility::Unknown` is treated as irreversible.** The two mistakes are
not symmetric — calling a reversible act irreversible costs one interruption;
calling an irreversible act reversible costs somebody's money. So an egress
nobody has classified gets the cautious answer rather than the convenient one,
and `EgressEffect::new` takes reversibility as a required argument so it cannot
be shipped by not thinking about it.

★★ **`Reversible` carries the compensating act.** "Undoable" with no undo named
is precisely the claim an auto-retry would be relying on. And even a safe retry
is bounded: undoable is not free, and an unbounded loop against a receiver that
is down is a different kind of harm.

★★ **A reversible live egress needs no approval token, on purpose.** The human
gate exists because an act cannot be taken back; asking for approval on one that
can would train people to approve without reading, which is how a gate stops
being a gate. A token that is offered and *wrong* is still wrong either way.

**Finding — the refusal that could not exist.** The obvious shape was an enum
with two arms, "you did not approve it" and "your approval is wrong". The first
is unconstructible: `EffectClass` has no *live and unapproved* variant, so no
caller can reach the gate in that state. Writing the arm anyway would have put a
refusal in the vocabulary that no input can produce — a check that reads as
performed while nothing is checked. Removed; `admit_egress` returns `TokenError`.

### `feat/strong-admit` — merged into `dev` 29 Aug — **CON-10 + CON-4 closed**

**Off:** `dev` · gate green (core 1,679 · host 245).

**`kernel.rs` existed and nothing ever admitted against it.** The viability
kernel was computed, tested and unused — a fixed point nobody asked a question
of. The row is one call, and the call is the whole point of having built it.

**A monitor enforces exactly safety, never liveness** (Schneider, CON-4), and
that is not a limitation to route around — it is what a monitor *is*. "Stays in
`V`" is checkable one transition at a time; "a move always exists" is a claim
about every future, and no single transition can witness it. So the weak gate
admits states with no way out: every pocket funded, every rule satisfied, and no
sequence of legal moves that keeps it there next month. Substituting the kernel
moves the refusal to the last moment it can still help.

**Findings — three, and each is a place a bool would have lied:**

★★★ **A horizon kernel admits and says it is provisional.** `Viab^H` is a
*superset* of the true kernel: surviving `H` steps is easier than surviving
forever, so a state inside it may still be doomed at `H+1`. Returning
`Survivable` on it would be the exact overstatement that makes a strong gate
*worse* than a weak one — a confident yes into a corner. `ProvisionallySurvivable
{ horizon }` is a third outcome and the honest one.

★★★ **An empty kernel admits.** No state surviving says the model is wrong far
more often than it says the household is doomed, and refusing every move on it
would freeze somebody's money over an analysis nobody checked. `Unknown` admits
for the same reason: the weak gate has already had its say, and this layer only
ever *adds* a refusal it can justify.

★★★ **What the row buys is countable, not assumed.** `doomed_but_viable()`
returns the states the weak gate admits and this one would not. If it is empty on
a household, the kernel is buying nothing there and the cost is not worth paying
— which is a real answer, and better than inheriting the assumption that an
expensive analysis earns its keep. The test fixture puts a `trap` state inside
`V` with a positive balance and one legal move that leaves it, and asserts
separately that the **weak gate really would have admitted it** — otherwise the
whole layer would be proving nothing.

### `feat/sigma` — merged into `dev` 29 Aug — **SUS-1 + SUS-4 closed**

**Off:** `dev` at `fb494a0` · gate green (core 1,670 · host 245).

**Every component existed; the object did not.** `boundary.rs` is `B`,
`schema.rs` is `S`, `region.rs` is `V`, `Definition` is `T`, `rollup.rs`
composes — five real things, assembled by whichever caller needed them in
whatever combination it wanted. A household was a `Definition` here, a `Region`
there, and a list of children somewhere else, and **nothing anywhere held the
claim that those are one thing.**

★★★ ***Components of a Sustain are Sustains* only becomes true when one type
says so.** Two types — one for "a Sustain", one for "a Sustain that has
children" — would put a ceiling in the model that the world does not have. A
village adopts into a county by exactly the call a habitat joins a household by.

★★★ **No privileged top is structural, not a convention.** There is no `Root`
type and no `is_root` flag: a root is one nobody has adopted yet, and a test
asserts a household is the *same value* alone and adopted.

★★ **A cycle is unrepresentable** — children are owned, so a Sustain cannot
contain itself and the compiler refuses to build one. Every other holon walk in
this codebase carries a visited-set; this one has nothing to guard against.

★★ Type-checking recurses, and a finding is **named by the Sustain it came
from**: a tree of thirty households reporting "invalid" is a report nobody can
act on. It checks the holon path too, so a household whose total ranges over a
dimension no habitat has is caught — that one reads as *a household with no
money* rather than as a mistake.

**SUS-4 closed by verification, not by flipping a row.** Its two open halves were
*deterministic* (now `canonical.rs`, SUS-15) and *evolvable*, which pointed at
M-EDIT — and M-EDIT is complete every row: the append-only definition DAG, `μ`
with Expand–Migrate–Contract, and the stranding check. The pointer outlived the
work it pointed at.

### `feat/outbox` — merged into `dev` 29 Aug — **ING-10**

**Off:** `dev` at `a750e61` · gate green (core 1,659 · host 245).

★★★ **`Unknown` is the whole reason this module exists.** A request that times
out has told you nothing — the engine may have committed it and lost the reply,
or never seen it. Treating that as failure drops real transactions; treating it
as success loses them just as thoroughly and more quietly. Keeping it and
sending again is safe **only** because capture is idempotent, and that is what
idempotency was for.

★★★ **An ack means durably committed, not received.** A server that
acknowledges on receipt and then crashes has told a phone to forget something
that exists nowhere — and the phone is the only other copy.

★★★ **Jitter without randomness, and it is the better answer.** The core is
deterministic and has no random source, so the wait is derived from the
message's own id: two phones retrying the same failure do not thunder together —
the usual point of jitter — *and* the schedule is reproducible, so a support
question about why something retried at a particular moment has an answer. Full
jitter (`[0, exponential]`), because two senders that back off to the same
ceiling still collide if they both wait the ceiling.

★★ **Queue depth is the leading metric** because it moves first: on the first
failed send. Staleness cannot show until a whole expected interval has passed,
and a missing transaction shows only when somebody goes looking. And "hardest to
deliver" is by **attempts**, not age — age says how long ago it arrived, attempts
say how hard it has been to deliver.

★★ A capture keeps its id across every retry. A new id per attempt would turn
at-least-once into as-many-times-as-it-failed.

### `feat/liveness` — merged into `dev` 29 Aug — **ING-8**

**Off:** `dev` at `5ebfad8` · gate green (core 1,648 · host 245).

**A capture path that dies is silent, and silence is also what a quiet Tuesday
looks like.** A revoked SMS permission, a rule that stopped matching, a bank that
changed its sender id — every one presents as *no new messages*. Nothing else in
the system can notice, because the failure is an absence.

★★★ **"Never guess a cadence" is the row's own constraint, and it is structural
here.** `learned_theta` returns `None` below five gaps: three observations are
three numbers, not a distribution, and a threshold from them is an alarm about
last week wearing a threshold's clothes. So a source nobody has watched long
enough is **unknown** — a third answer. Healthy would hide a dead capture path;
stale would cry wolf about a source that is simply new.

★★★ **A heartbeat converts a statistical detector into a deterministic one.**
`gap > h + δ` is a fact, not an inference — no percentile, no history, no
warm-up, watchable from the first message. Where both exist the promise wins.

★★ **Nearest-rank, not interpolation**, so the threshold shown is a gap this
source really had rather than an average of two it never did. And the grace `δ`
is required rather than defaulted — a declared cadence is a promise about intent,
not about the network, and where "late" begins belongs to whoever declared it.

★★ *Seen when it speaks, not when it parses* was **already right** in the host —
verified rather than assumed, and now asserted by a test. A message nobody could
read still proves the phone is on; counting only successful parses would send
somebody to check the phone when the answer is *the format changed*.

★★ A negative gap is discarded rather than recorded: a clock that moved backwards
is not a cadence, and it would poison the percentile with a number no source ever
kept.

### `feat/event-time` — merged into `dev` 29 Aug — **ING-13**

**Off:** `dev` at `55bfc8e` · gate green (core 1,637 · host 240).

**The patterns had been capturing `date` and `time` all along and throwing them
away.** 32 fields across 20 rules are kept now; nothing about what any rule
*matches* changed.

**Without `t_event`, skew was not merely unknown but unmeasurable.** Every
capture carried only the moment the phone read it — so a backfill of two thousand
texts stamped a month of spending with one afternoon, every window closed on the
wrong side, and nothing in the system could tell, because there was no second
reading to disagree with the first.

Three ways to be wrong by a lot, each pinned by a test:

- **`12 AM` is midnight and `12 PM` is noon**, and the obvious arithmetic gets
  both backwards. A twelve-hour error puts an evening payment on the wrong day.
- **`20/7/26` is day-first**, settled by real samples where 20 cannot be a month.
  The other reading moves an event by up to eleven months — and only on the
  ambiguous days, so it fails invisibly for two thirds of every month.
- **An unreadable date yields `None`, never *now*.** *Now* makes skew exactly
  zero — the one value that looks healthy — for precisely the messages nobody
  could read a time from.

★★ The UTC offset is the **host's** declaration about its own senders. A message
says half past four; it does not say half past four *where*, and the core carries
no locale data to guess with.

★★★ The conformance vector's divergence block now names the two extra fields,
and the replay allows extras **only where the block names them** — a field nobody
declared appearing in a parse result would be a change nobody reviewed, and
noticing that is what a vector is for.

### `feat/unscored-dimensions` — merged into `dev` 29 Aug — **SUS-17**

**Off:** `dev` at `b57b151` · gate green (core 1,626 · host 237).

**Goodhart's law is not about the measured dimension going wrong — it is about
the unmeasured ones.** Optimise the pockets and the inventory quietly empties;
optimise the total and one member's tab quietly runs. The dimension nobody
scored is the one that moves, precisely because nothing is looking.

★★★ **`goodhart.rs` deliberately did not build the auto-emit half, and its
reason was right:** emitting a *bound* needs to know what bound, and a
fabricated threshold over a dimension nobody watches is a confident number from
nowhere. That reasoning stands and is honoured here.

★★★ **But there is a bound that is not invented: the household's own declared
type.** `Number { lo: 0 }` is the definition saying this does not go below zero.
Emitting an invariant from it is *using their number*, not choosing one — and it
converts a bound the schema **describes** into a bound the gate **holds**.

**Everything else is named, not bounded.** Unscored *and* unbounded dimensions
are listed rather than guessed at: filling them with a floor would replace a
visible gap with an invisible wrong answer.

Two judgments: **a label gets no bound** — it cannot drift, and a rule that can
never fire makes the list of guards look longer than the protection is. And **an
open map is left to a quantifier rather than enumerated** — a pocket is named by
a person, so rules over today's keys would silently fail to cover tomorrow's,
which is the worst kind of gap because the list looks complete.

★★ The emitted invariants are parsed and *evaluated* in tests, not just
generated — a guard the evaluator cannot read is a guard that silently never
holds.

### `feat/runway-bound` — merged into `dev` 29 Aug — **SUS-11 + IMM-2 closed**

**Off:** `dev` at `2450dc2` · gate green (core 1,616 · host 237).

**`⌈W/α⌉` — how many turns until this is over.** Lyapunov guarantees each step
moves toward the region; it does **not** guarantee arrival. A loop that closes
half the remaining gap every turn descends forever, arrives never, and passes
the stability check at every single step. The bound is what turns *this is
improving* into *this ends*.

★★★ **`Unbounded` is named rather than reported as a very large number.**
"Never, at this rate" is the actionable fact — it says *look for a bigger move*.
A big number reads as patience being enough.

★★ The projection is computed from the descent **actually achieved**, not from a
rate somebody declared and nothing checks. And `Arrived` is distinct from
`Steps(0)`: "nought turns away" and "already here" are the same number and
different facts.

**IMM-2 closed by CON-8's module rather than by a second one.** The row is the
same claim from the Immune side, and the article is explicit that there is *no
separate layering construct* — so building one would have been the deviation.
Verified rather than flipped: `admissibility.rs` is monotone and
order-independent by test, and person-first survives as refusal-only-on-a-newly-
caused breach.

### `feat/durable-calls` — merged into `dev` 29 Aug — **SUS-6 + the wire "flake", fixed**

**Off:** `dev` at `9ceb61c` · gate green (core 1,610 · host **237**, in 4.1 s).

**`semantic::replay_under` had nothing real to read.** It re-runs the logged
Enzyme **calls** under a chosen definition — that is what makes *would this
still have been admissible under `D′`?* answerable, and what EDIT-11's stranding
check stands on. It needs `(operator, params)`. The durable log recorded the
operator and the resulting mutations, so **half the call was durable** and the
mechanism could only be exercised from hand-built fixtures.

`LoggedEvent.params` is additive and `Option`, with `#[serde(default)]`, so
every line written before this deserialises byte-identically — the row's own
"addable later without migration" turned out to be true.

★★★ **`None` means *we do not know what it was called with*, which is genuinely
different from *called with nothing*.** `enzyme_calls` returns `(calls,
skipped)` and never hands on an empty map: an Enzyme called with nothing is a
different call, and replaying it would be reporting confidently on something
that did not happen. A caller can say *these are the ones we could check* rather
than implying it checked everything.

★★ Genesis and transfer legs carry `None` deliberately — neither is an ordinary
Enzyme call, and filling them with an empty map would claim they were called
with nothing.

**The `wire` "flake" was a real bug, and it is fixed.** Two of its tests failed
under a full parallel run, then four as the suite grew, and all eleven passed in
isolation — taking **342 seconds**. The cause: each wire test unlocks **five**
identities at PBKDF2's 600,000-iteration production cost, then talks over a
socket with a **20-second IO timeout**. Under parallel load an unlock takes
longer than the peer will wait, so the handshake times out mid-flight. It fails
more as the suite grows, which is why it looked like a flake and was not one.

★★★ Lowering the timeout was the wrong fix — 20 seconds is a real protection
against a peer that stalls, and weakening production to make a test suite
comfortable is backwards. The KDF cost is `cfg(test)`-lowered instead, with
OWASP's floor kept as its **own constant and asserted** in `identity.rs`, so the
number that ships is still guarded. The wire tests are about the wire.

**The host suite went from 250+ seconds to 4.1.** It had been spending
essentially all of its time deriving keys.

### `feat/domain-map` — merged into `dev` 29 Aug — **SUS-16**

**Off:** `dev` at `4ea27d4` · gate green (core 1,600 · host builds clean).

**`cynefin.rs` had the taxonomy and nothing that could produce a reading.** It
could say what each domain presupposes and refuse a mismatched operative — and
its own doc said plainly that inventing a classifier would be *a confident
classification from nowhere*. The article says where the reading comes from: the
household declares it, **per region of its own state space**.

**A household is not in one domain.** Its rent is clear — there is a best
practice and it works. A new side business is complex — you probe, you see what
happens, you amplify what worked. Conant–Ashby's *a good regulator is a model of
the system* is exactly the claim that a one-domain model of a many-domain
household regulates it badly.

★★★ **A state no declared region covers is DISORDER, never `clear`.** That is
the dangerous default: *clear* licenses best practice, so an unrecognised
situation would be met with the method that presupposes it is already
understood. Disorder means *we do not know which rules apply*, and saying so is
what lets somebody find out.

★★★ **Two declarations that disagree are disorder too** — genuinely "we hold two
incompatible models of this situation", which is the failure mode §X names.
Picking the first by declaration order would hide a real contradiction behind a
confident answer. Two that **agree** are not a conflict, so a household's regions
need not be a perfect partition.

★★ A region naming a dimension that does not exist is caught at load. Otherwise
it silently never matches and the household sits in disorder for a reason nobody
can see.

### `feat/cross-source-correlation` — merged into `dev` 29 Aug — **ING-12**

**Off:** `dev` at `ec6057a` · gate green (core 1,591 · host 230).

**One transfer, two senders, one fact.** The raw-text dedup cannot see this by
construction — it keys on the text, and the two texts are genuinely different.
Both get captured, correctly, and applying both puts forty thousand shillings
into the books twice.

**The finding, and it is the serious one: the host was already joining these —
on the REFERENCE ALONE.** A reference is not a fact. Codes are allocated by
different systems and collide, and when they did, a real separate transaction
was silently treated as already-applied and **vanished without trace**. Nobody
finds out, because the whole effect of the join is that nothing is shown. The
key is reference **and** amount now.

An unknown amount falls back to the reference rather than refusing to join —
narrowing further would miss real pairs for messages whose rules extract no
amount, and the failure directions are not symmetric: a **missed** join shows a
duplicate a person can undo; a **wrong** join hides a transaction nobody knows
to look for.

★★★ The core module **surfaces rather than suppresses**, naming both messages.
A false positive that asks costs one tap; a false positive that hides costs a
transaction and the trust in every other number.

Two judgments: **no text-hash fallback across sources** — two descriptions of
one event have different text by definition, so a text key would be exactly
backwards here; and **opposite directions are evidence FOR**, because one side
sent and the other received is what a single transfer looks like from two
vantage points.

### `feat/control-system` — merged into `dev` 29 Aug — **CTL-1 · M-CTL COMPLETE**

**Off:** `dev` at `b3f448d` · gate green (core 1,580 · host builds clean).

**Every piece of the loop existed; the loop did not.** `region.rs` gave `e`,
`controller.rs` gave Lyapunov and the Sheridan dispatch, `ooda.rs` gave the
phases — and each caller assembled them in whatever order it chose. Four callers
is four loops, and the day two of them disagree about whether to measure before
or after the candidate, nobody can say which one is the Controller.

★★★ **The Actuator is deliberately not here.** §I closes the loop through a
*human*, and ADR-0001 keeps I/O out of the core. `step()` computes what the loop
would do and returns it; acting is the host's and authorising is the person's. A
step that could act would close the loop through itself, which is the one shape
supervisory control is defined in opposition to. A test asserts the plant comes
back untouched.

Three judgments: **a move that increases `W` is not a candidate at all** —
Lyapunov is the definition of a good decision, not a ranking, and offering a
worsening move would be offering to make things worse and calling it an option.
**Inside the region it proposes nothing**, because a controller that acts when
nothing is wrong is the over-corrector damping exists to catch. **A tie goes to
declared preference**, never to whichever candidate the vector happened to hold
first.

★★ *Nothing helps* is an answer, not an error. Reporting it as a failure would
push a caller toward acting anyway; saying so plainly is what lets a person go
and find something that does.

**This closes M-CTL.** The module header still read *NOT STARTED* while most of
its rows were ✅ — corrected, with the original note kept for the record.

### `feat/canonical-hash` — merged into `dev` 29 Aug — **SUS-15 + CON-8**

**Off:** `dev` at `6a728b2` · gate green (core 1,571 · host 223).

Two rows, landed together because the second was found while wiring the first.

**SUS-15 — `H(s) = hash(canonical(s))`.** `fold(log) == cache` is the law the
whole engine rests on, and checking it meant comparing two whole state trees.
Across a peer link that is not a question anyone can ask: you cannot send a
household's entire finances to find out whether two nodes agree, and a node that
answers *mostly* has answered nothing. Sixty-four characters either match or they
do not.

★★★ Sorting object keys is not tidiness — this crate builds its `Map` with
`preserve_order`, so hashing insertion order would report a household as diverged
**from itself**. Numbers go through the engine's own `num` rule for the same
reason. Array order is preserved, because in a list order IS the value.

★★ `sha2` is the first hashing dependency, named rather than slipped in: `H(s)`
is compared across peers that need not trust each other, and a hash somebody can
collide on purpose verifies nothing.

**CON-8 — `⋀_{H∈path} admit_H`, the full path.** Only the immediate parent was
ever asked, so a village-level rule two edges up did not apply — silently, with
nothing saying a level had been skipped. A holon three deep is not exotic; it is
the shape the model is named after.

Judgments kept: **refusal only on a newly-caused breach** (a household already
over its limit freezing every member would punish exactly the household that
most needs its members able to act); **advisory by default**; **every level
reported**, not just the first to refuse.

★★★ **The finding:** an unreadable rule evaluates false both before and after,
so it looked exactly like a pre-existing breach and was waved through as *not
newly caused* — a gate failing open at the worst possible moment, right after
somebody mistyped a law. The reading is three-valued now: held / failed /
**unreadable**. DSL-5's *a broken document is not a violated law* turns out to
apply here too, and a test caught the conflation rather than a reading of it.

### `feat/dsl-mdl-lift` — merged into `dev` 29 Aug — **DSL-15 · PHASE 4 COMPLETE**

**Off:** `dev` at `b29907e` · gate green (core 1,548 · host 223).

**Learning is compression, made arithmetic.** A household writes the same rule
into six pockets — *never let this go below zero* — and six copies is six places
to be wrong. The count IS the evidence: a shape written once is somebody's
particular rule; a shape written six times is a concept they have and have not
named.

Judgments:

- **`n` is the whole formula, not a rounding detail.** A pattern used twice
  usually costs more than it saves, because the definition has a length of its
  own. The formula says so instead of leaving it to taste, and the threshold
  falls out of the numbers rather than being chosen. A test pins that two is not
  yet a concept and six is.
- **Length is counted over the typed AST, never the text.** Two rules differing
  only in spacing are one rule; counting characters would make the formatter
  part of the arithmetic.
- **A call is charged two nodes, not zero.** Pretending a call is free is how
  every abstraction looks worth it.
- **A proposal is not an edit.** `ΔL > 0` says a lift would be shorter — not
  that the household wants it, that the name is good, or that six pockets
  rhyming this month is a concept rather than a coincidence. A person and the
  migration predicate stand between, the same division `learned.rs` keeps.

★★ The hole is the **open-map key** — a pocket's name is a value the household
chose, the segments around it are structure. Holing every segment would make
every rule identical; holing none would make a lift impossible.

**★★★ This closes Phase 4.** Every DSL row landed on 29 Aug. The one declared
residual is DSL-12's `imports` + enforced versioning, which has no mechanism
anywhere yet.

### `feat/dsl-surface` — merged into `dev` 29 Aug — **DSL-14**

**Off:** `dev` at `616a48f` · gate green (core 1,538 · host 223).

**JSON was the only way to write an Enzyme, and JSON is a terrible thing to ask
a person to write a rule in** — quoting, commas, nesting, and a shape that
obscures the one thing that matters: *when this, do that*. The row sits last in
the phase for a good reason: a friendlier surface built before the AST was
settled would have been a friendlier way to write the wrong thing.

**The theorem, executed:** text and builder produce a value that is `==`. Many
surfaces over ONE core is a feature; many surfaces over two cores is two
languages wearing one name, which is exactly what DSL-6 forbids. And
`parse(render(e)) == e` runs, which is what makes this a surface rather than an
importer.

★★★ **The guard is handed to the one predicate parser, untouched.** A second
predicate parser here would be the two-evaluator failure in miniature, arriving
silently — the same words meaning two things depending on which file they were
written in.

Judgments: **line-oriented, no nesting** (total by construction, and every error
gets a real line number — "invalid syntax" for forty lines makes somebody re-read
all forty); **quoting is the only thing separating a literal from a path**,
because guessing would make one string mean two things on one line; and
**letting money go below zero has to be typed out** (`or below zero`) rather than
defaulted.

**A real defect found on the way:** `ParamDecl::name` was `&'static str`, so a
parameter named at run time could only be obtained by **leaking** it. A person
editing their own rule ten times would leak ten times — a defect that grows with
use rather than one a test sees. It is a `Cow` now; every compile-time
registration still borrows.

### `feat/dsl-authored-enzymes` — merged into `dev` 29 Aug — **DSL-13 (the large one)**

**Off:** `dev` at `2e75491` · gate green (core 1,525 · host 223).

**A definition named Enzymes; it never said what they DO.** `⟦operator⟧` was a
name lookup, so the only people who could add a capability to a Sustain were the
people who could compile one — and the language a household is supposedly
written in could not express its own rules.

`⟨g, e, ε, μ⟩` is now data: guards in the same language invariants use, actions
over declared paths, declared emissions, declared movements.

**It runs INSIDE the gate**, through one dispatch at the single existing call
site — same admission, same invariants, same double-entry reconciliation. Four
tests pin that: an authored Enzyme commits and folds; its guard refuses and
leaves no trace; the household's own invariants still refuse it (authoring an
Enzyme does not author a way around the law); and one that lowers a cash account
without declaring what it moved is refused as a single-sided entry.

★★★ **Its effect summary is DERIVED from the actions, so it cannot lie.**
`obligation` is explicit that a native summary is a *claim about a body* which
may be wrong. An authored Enzyme has no separate body to disagree with, so
composition reasons about what will actually happen.

Two deliberate absences: **no arithmetic in expressions** — it lives in the
actions, which is exactly how `StateAccessor` already works, so this is the
engine's own shape rather than a second one; and **no control flow** — sequencing
belongs to `dag.rs`, where it is checked, and an effect that could branch would
put an unchecked second sequencer inside the one place that must stay total.

★★ `run` still holds a real function for an authored Enzyme, and it refuses. If
the dispatch is ever got wrong the result is an honest error rather than a native
body running with an authored Enzyme's parameters. A test calls it directly.

### `feat/dsl-spec-validator` — merged into `dev` 29 Aug — **DSL-12 (mostly)**

**Off:** `dev` at `3862ddb` · gate green (core 1,509 · host 223).

**One door, and everything a definition names goes through it.** `typecheck`
checked the invariants — one of several things a definition *names*. It also
names Enzymes it may run and aggregates it will total, and neither was bound to
anything.

**Same class of failure as DSL-5, in a different place.** A spec naming an Enzyme
this engine does not provide loaded perfectly; the first person to try it was
told the operator *is not available on this sustain* — which sounds like a
permission, reads like a rule, and is a typo in a document.

Judgments: **names first, then types** — reporting a mistyped rule while an
unknown Enzyme is outstanding sends somebody to fix the wrong thing. **Every
finding is collected**, because the second error is often what explains the
first. **Duplicate ids are findings, not tie-breaks** — somebody meant two things
and one is silently unreachable; picking a winner would be this module deciding
which of their rules to discard. And an **aggregate id that shadows a real
dimension** is refused: a parent rule reading that name would silently get the
total instead.

**Residual, declared rather than quietly dropped:** `imports` + enforced
versioning, which has no mechanism anywhere yet, and widget-name binding, which
`WidgetSet::load` already performs at its own door.

### `feat/dsl-holon-typing` — merged into `dev` 29 Aug — **DSL-10**

**Off:** `dev` at `169ebb2` · gate green (core 1,500 · host 223).

**A parent and its children are one program.** A parent declares *sum every
child's `finances.liquid.balance`* and writes rules about the total. Three things
must agree: the child must HAVE that dimension, it must be a kind you can SUM,
and the parent's rule must treat the result as what it is. Check the parent alone
and all three can be wrong at once with nothing to report it.

**Both failures are silent, which is what makes author-time work worth it.** An
aggregate over a dimension no child declares does not crash — it totals **zero
contributions**. Summing a string does not crash — it skips every child as
unreadable and reports the same zero. Both read exactly like a household that
genuinely has nothing, and there is no later moment at which anyone can tell.

Judgments: **every child is checked, not a sample** — a holon of habitats and a
shop is ordinary, and checking one is how a roll-up silently drops the members
that differ. **`COUNT` is exempt on purpose** — how many children have a label is
a real question; what their labels sum to is not. **An empty holon is not an
error** — a holon is declared before it is populated.

★★ `AggregateDecl::segments()` was exposed so the check resolves against a
child's schema with the same segments roll-up will read with, rather than a
re-parse free to disagree.

### `feat/dsl-typed-overlay` — merged into `dev` 29 Aug — **DSL-9**

**Off:** `dev` at `6f8bc11` · gate green (core 1,491 · host 223).

**`ρ`: you hand a person the overlay, never the definition.** A definition says
what a Sustain *is*; an overlay says only what some of its values *are*. A
surface that had to hand out the definition would have to decide, per field,
whether this person may change this thing — a judgment made in the UI, where it
cannot be checked. As a type, the check runs once before anybody sees a form.

**The finding, and it is the interesting one: per-path typing is not enough.**
Every path in an overlay can be a declared dimension of exactly the right type
and the RESULT can still be malformed — setting one field of a new open-map key
invents half a pocket, a name with no allocation. The shape changes anyway, one
key at a time, which is precisely what the Curated UI leans on an overlay not to
allow. A test found it, not a reading. `apply` now validates the whole candidate;
the per-path judgment stays because it gives the better message when it is the
one that fails.

Two more judgments: **right-biased** because somebody who types a value, thinks
again and types another has changed their mind — left-bias would make the first
edit of a session unchangeable. And **type-correct is not the same word as
permitted**: an overlay putting a balance below zero is perfectly well-typed and
still refused by the gate. A test pins that distinction.

★★ `predicate::parse_path` was exposed rather than splitting on `.` somewhere
convenient — a second spelling of what a path means is how two parts of one
language drift, silently, until a path resolves differently in an invariant than
in the surface that wrote it.

### `feat/dsl-typed-params` — merged into `dev` 29 Aug — **DSL-8**

**Off:** `dev` at `5e2aaa7` · gate green (core 1,476 · host builds clean).

**`Def(θ)`: a definition is a typed function into Sustains, checked once.**
Definitions had no parameters at all, so there was nothing to check and
substitution was textual — a token nobody supplied **survived as the literal
string** and landed in real state. That is the `role_in_family` bug, and it is
now caught at **author time**: the mistake was never in anybody's argument, it
was a template referring to something it never declared. Textual substitution
has no step at which to notice.

The substitution lemma is asserted as a **property**, not claimed: one check,
then many θ, each result conforming to the schema without being re-checked.

Three judgments:

- **A whole-value `"{{limit}}"` keeps the parameter's own type**; an embedded
  `"pocket for {{name}}"` interpolates. A template that stringified the first
  would fill a number dimension with a word and the schema would refuse the very
  Sustain the template exists to build.
- **"Checked once" has to cover the RESULT**, so the template's own shape is
  checked against the schema using a canonical witness per declared type — not
  the defaults, which a caller is free to replace.
- **An argument nobody declared is named, not ignored.** Silently dropping it is
  how a typo in a caller goes unnoticed for months.

★★ `schema::conforms` was extracted rather than written fresh, so a parameter's
argument is checked by exactly the rule a dimension's value is.

### `feat/dsl-type-judgment` — merged into `dev` 29 Aug — **WBD Phase 4 opened**

**Off:** `dev` at `130176c`
**State:** merged, gate green (core 1,463 · host 223).

**DSL-5 + DSL-1's type-check half** (GENOME §V), taken together because they are
one tree-walk: the judgment, and the door it guards.

**The gate has three outcomes — refuse, clamp, defer — and no fourth for "this
rule is broken".** So an untyped mistake arrived dressed as a refusal: a rule
comparing a pocket's NAME to a number reported that the household had broken a
law, and somebody would go looking for money that never moved. `editing::typecheck`
**bound names without typing anything**, so `label >= 5` loaded cleanly.

Three judgments the article settles and one it does not:

- **Ordering needs the same type; equality is total.** `a == b` across two types
  is honestly `false` — they are different values. `a > b` has no answer at all.
  Refusing both would outlaw a perfectly good "is this empty" test. My first
  pass applied the type rule to both and a test caught it.
- **`Any` and an undeclared param are compatible with everything.** `Any` exists
  so a schema can be adopted one dimension at a time; a checker that complained
  about silence would make adopting it cost a full re-declaration first.
  Unstated is not the same as wrong.
- **An undeclared path makes no type complaint.** `bind` already reports it, and
  a person who reads two complaints for one mistake learns to read neither.
- **Not settled, so not invented:** whether `notes == 5` deserves an
  always-false lint. That is a lint, not a type error, and the article does not
  ask for it.

★★ `type_at` reuses `Schema::resolve` rather than growing a second path-walker.
The first draft had its own, which would have been a second answer to a question
the binder and the evaluator already agree on, free to drift from both.

### `feat/parser-signs` — merged into `dev` 29 Aug — **money model §8 COMPLETE**

**Off:** `dev` at `80f5709`
**State:** merged, gate green (core 1,445 · host 223).

**§6's settled signs, implemented.** Nine instrument shapes that all asked a
person now route themselves: Fuliza borrow / interest / repayment, M-Shwari in
and out, cash withdrawal at a till and at an agent, Pochi in and out.

**The finding underneath it: an overdraft was inexpressible.** No operator
anywhere wrote `finances.liabilities.*`, so borrowed money could only be filed
as income — which overstates the household by the whole of what it owes.
`budget.borrow`, `budget.charge_debt` and `budget.repay_debt` are the first
operators that write it. A borrow is one transaction with **two positive
postings** (§3): the money arrives and the obligation appears, and a test pins
that borrowing leaves the household's position exactly where it was, because
borrowed money is not wealth. The charge for borrowing gathers in a pocket of
its own (§6.2) so the running cost of an overdraft is visible over time.

**A money-safety test had to be restated, and that deserved care.** The old rule
was *"the only auto-applied operator is income"* — true, and the right rule,
while income was the one unambiguous shape. The rule it was standing in for is
**nothing files money into a CATEGORY without being asked**, and that has not
moved: `budget.spend` and `budget.allocate` remain un-auto-appliable by any
rule, and a payment to somebody outside the household still always asks. What
the settled shapes have in common is that no category is involved — an overdraft
names its lender, a transfer has the household at both ends, a withdrawal moves
money to your own pocket. Asking which pocket a cash withdrawal belongs to is a
question with no true answer. The rule is now asserted directly instead of
through a proxy that had stopped holding.

**A second finding, from the same failure:** the transducer's `Real` operator
universe — the thing `typecheck_rule` was checked against — was a **stub that
knew one operator**. Every rule naming an operator that does not exist, or
handing it a parameter it does not declare, would have passed. It is the real
registry now.

**Divergence recorded, not papered over.** These nine are mapped in Rust and
`parsed_unmapped` in the Python reference, which cannot express an overdraft at
all. `conformance/vectors/transducer.json` carries a `divergence` block, the
replay skips those cases **by name**, and a test fails if the block disappears.
`conformance/README.md` §Recorded divergences has the entry.

**§8 is now complete** — 1 double entry, 2 parser signs, 3 person accounts,
4 realignment, 5 operative DAGs, 6 net worth. The one piece left open is
graduation to a real child Sustain, which needs `holon.create_child` and is
noted under `feat/operative-dag`.

### `feat/net-worth` — merged into `dev` 29 Aug

**Off:** `dev` at `ca90fb7`
**State:** merged, gate green (core 1,435 · host 223). APK built and verified;
**not installed — the phone was off USB for this whole stretch.**

**Money out is not money gone.** A week's shopping leaves the account and
becomes food in the cupboard; a payment to somebody who will pay it back leaves
the account and becomes a claim. A position that counted only cash calls both a
loss, and a household that shops well looks identical to one losing money —
which is the exact failure `inventory` was added to fix, finished here.

`ledger::position` reads four sides off state: cash in accounts, things held,
owed to you, owed by you. Two judgments:

- **Ordinary pockets are not counted.** An envelope is a view of money already
  in an account, so adding it would count the same shilling twice and the total
  would grow every time he budgeted — the opposite of what budgeting does. Only
  person pockets contribute, because those are claims rather than envelopes, and
  `is_person_pocket` is what tells them apart.

- **Money no account has claimed is still money.** Leaving the unattributed part
  of liquid out would understate the position by exactly the amount nobody had
  got round to filing.

Three tests exist to pin the invariant that makes it worth having: lending,
shopping and being repaid all move the position **nowhere**. If any of them
moved it, a loan would read as a loss and a repayment as income.

**The card draws four lines, never one number.** A single total hides the
distinction that makes it true, and rows at zero are not drawn — a household
with nothing lent out should not read past a row of noughts.

### `feat/operative-dag` — merged into `dev` 29 Aug

**Off:** `dev` at `1b7bcf0`
**State:** merged, gate green (core 1,428 · host 223). APK built and verified; **not
yet installed — the phone was disconnected from USB when the build finished.**

**The keystone: operatives are graphs of operator calls, and the graph is data.**
Every deciding step was already an operator — `vendor.identify`, `vendor.suggest`,
`budget.spend` — called from a screen in an order written in a host function. An
operative that lives in a screen cannot be inspected, cannot be checked before it
runs, and cannot be changed without editing the app.

`compose.rs` had held Hoare sequencing since it was written and **nothing had
ever called it**. `Dag::typecheck` now runs every root-to-leaf path through
`Pathway::try_chain`, so a chain whose third step can never fire is refused when
the graph is *written*. `OperatorMeta::as_step` is the bridge that was missing.

Three findings, each from testing rather than reading:

- **`vendor.remember` could never commit on a real household.** `vendors` was
  DECLARED in the schema and never seeded into the opening state, and
  organisational closure refuses an operator that introduces a top-level
  dimension — correctly, since that changes what the Sustain *is*. So the memory
  had no choice but to live in a side file. That is the actual root of the
  double-write, not carelessness. Fixed by seeding `vendors: {}`, plus a logged
  one-time backfill at `World::open` for households that already exist —
  a shape change made at the level shape changes belong to, never behind the
  log's back.

- **Ordering by topological position is not a promise.** Two nodes with no edge
  between them have no order; a binding that reads a sibling works for exactly
  as long as the sort keeps favouring it. The check is ancestry now, so only an
  edge counts.

- **My own test was wrong about vendor matching.** `NAIVAS` and
  `NAIVAS SUPERMARKET` are deliberately different vendors — dropping a word is
  the conservative half of the only tradeoff `vendor_key` makes, and the test
  was asserting the loose behaviour we specifically refuse.

**Mentor and Attaché read the same `suggest` step opposite ways** — Mentor acts
when the household already knows a counterparty, Attaché when it does not — and
a test asserts they never both act, because if that ever stopped being true one
would be silently duplicating the other on real money.

**Honestly missing:** graduation into a real child Sustain (§4) needs
`holon.create_child`, which this engine does not have. Attaché takes a
counterparty as far as linked-and-known and stops. There is no `parse` node
either: the transducer runs at the ingest boundary and is not an operator.
Naming nodes for either would make the picture prettier and the graph
unrunnable.

### `feat/person-tab-display` — merged into `dev` 29 Aug

**Off:** `dev` at `3f5b78c`
**State:** merged, gate green (core 1,411 · host 220), installed and verified on device.

**A person pocket was drawn exactly like `food`.** It nets correctly and always
did, but a screen that draws a relationship identically to a category is telling
him they are the same kind of thing — and the first time that matters is the
moment he files a repayment as shopping.

**The two sides are DERIVED, never stored.** A pocket keeps one number, `spent`,
which already says where the tab stands. What it cannot say is how it got there:
KES 1,000 outstanding reads identically whether he sent 1,000 once or sent
40,000 across a year and got 39,000 back. `sustena-core/src/tab.rs` reads both
sides back off the log, because `state = fold(events)` means the history the
state was folded from is still there. Running totals kept beside `spent` would
be a second source that could disagree with the first, with no way to say which
was right — and a tab that appeared to begin the day it was linked.

Two judgments worth disagreeing with:

- **The tab is bounded on ONE side only.** Sending somebody more than the
  household set aside for them is a real departure, measured against their own
  allocation. Their owing HIM money is not — it is the tab doing what a tab
  does. A two-sided interval would have made every repayment read as a fault.

- **One tab on screen, not all of them.** Orchie's premise is an attention
  budget; a list of everyone he has ever paid is the flood it exists to prevent.
  The one shown is the relationship with the most money in play, either
  direction — measured, not guessed. Every linked pocket is still marked in the
  picker, so nothing is hidden, only unranked.

The card says **who owes whom in words**. A negative balance is a convention the
ledger uses and a person has to decode; getting the sign backwards on a screen
is how somebody pays a debt that was never theirs. The number appears masked
(`072···961`) — he linked it as an identifier, and the pocket name already says
which person it is.

### `feat/person-pockets` — merged into `dev` 26 Aug at `111435b`

**Off:** `dev` at `66af726`
**State:** merged, gate green (core 1,404 · host 214), installed and verified on device.

**A pocket can now be somebody, not just something.** `vendor.link_number` ties a
real phone number to a pocket, and from then on money to that number comes OUT of
that pocket and money from it goes BACK IN — one running tab that nets, rather
than a spend in one place and an unrelated lump of income in another.

Three findings, none of them visible from reading the code:

- **`budget.unspend` was reporting success on a mutation that never happened.**
  It called `state.decrement(..., allow_negative = false)` and discarded the
  `Result`. Where the decrement was refused, the operator still returned `ok`:
  the card said filed and the ledger said nothing. Found by a test that expected
  a number to move and watched it stay. Now the refusal is returned.

- **An envelope and a tab need different rules, and it is the pocket that
  decides.** Taking back more than ever went out is meaningless for spending and
  ordinary for a person — she sends first, or sends back more than she was sent.
  `allow_negative` now follows `is_person_pocket`, which is derived from a link
  existing rather than stored as a second flag that could disagree with it.

- **A mask and a key are written in different dialects.** Messages print
  `0726***961`; the key is the bare nine digits. Matching them straight fails on
  the leading zero alone, which would have made the link useless for exactly the
  messages it exists to route.

**The design call worth disagreeing with:** a receive from a linked number is
REDIRECTED automatically (it was already going to be applied — only *where* was
in question, and filing it as income while her tab still showed everything
outstanding made both halves wrong). A send is only PRE-FILLED and still waits
for his tap. Nothing new started applying itself.

**Left open:** the tab is a pocket like any other on screen. It nets correctly,
but nothing yet says "this one is a person" or shows the two sides separately.

### `feat/reclassify` — merged into `dev` 26 Aug at `30b0e34`

**Off:** `dev` at `4f69c7d`
**State:** merged, gate green (core clean, 202 host tests), installed

**There was no way to fix a wrong pocket.** "pick another pocket" existed only
inside the refusal branch, before anything was recorded, and once a spend landed
there was no list of recorded transactions anywhere in Orchie to reach it by.

`budget.reclassify` is a correction, never an edit — the original filing stays
and the move is appended after it, so the log says both. Semantically it is
`unspend(from)` then `spend(to)`, and it is ONE operator only for atomicity:
nothing runs two calls as a single gate decision yet, and a half-landed
reclassify would leave the money in neither pocket. **When the graph runner
arrives with transactional semantics this becomes a two-node path**, which is
the point of building it as an operator.

It refuses with the same numbers a spend does when the destination has no room,
so a screen can reuse the fund-or-backfill branch rather than growing a second
answer for the same situation. Assets from that purchase move with it; assets
from other purchases do not.

### `feat/vendor-operator` — merged into `dev` 26 Aug at `4f69c7d`

**Off:** `dev` at `0d700db`
**State:** merged, gate green, **not yet wired into the classify UI**

Three operators — `identify`, `suggest`, `remember` — because they are three
kinds of act, and one call would make a lookup and a write indistinguishable to
anything composing them. `identify` and `suggest` are read-only and are
operators anyway: a node in a DAG has to be one.

**Closes an audit deviation.** Merchant memory lived in a side JSON file beside
the log, so a rebuild could not reproduce it and no other Sustain could read it.
It is a `vendors` state dimension now.

**Open:** the classify UI still reads the old side-file history. Swapping it is
small but changes behaviour in the thing he uses daily, so it is deliberate work
rather than a drive-by.

### `feat/inventory-consume` — merged into `dev` 26 Aug at `aaf2586`

**Off:** `dev` at `d029d60`
**State:** merged, gate green (core clean, 197 host tests), installed

Buying rice left the household no poorer — cash became rice. Eating it is what
does. Until now the ledger called the purchase the expense, which is off by
however long the thing lasts: a month's shopping looks like a terrible week and
the week it is eaten looks free.

`inventory.consume` moves no money, for the same reason `itemize` does not. What
changes is what the household still HAS. Partial by design, since half a sack is
the normal case — an asset used in part keeps its identity and loses value,
because splitting it would multiply the list every time anyone cooked.

Consuming is not deleting: the asset keeps its name, pocket and purchase, so
"what did the food money buy" still answers after the food is gone. It leaves
the held total because it is no longer held, and stays on the record because it
still happened.

**UI ships whole-asset only.** Partial consumption is real and the operator
handles it; asking for an amount before the loop is proven would be guessing at
how he wants to say it. That is the next slice here.

### `feat/recoverable-skips` — merged into `dev` 26 Aug at `d029d60`

**Off:** `dev` at `e453a1c`
**State:** merged, gate green, installed

He waved both halves of a real self-transfer past impatiently and they were gone
for good. A shared transaction reference now brings a skipped message back.

**The line that makes it safe is which evidence may overturn a decision he
made.** Amount and merchant say two texts COULD be the same movement; a shared
reference says they ARE. Only the reference reaches a skipped message — the
amount-and-merchant fallback is filtered to messages still in the queue, in both
the transfer pass and the reversal pass. A promo, a balance notice, an advert:
none quotes a reference another message shares, so a genuine "not a transaction"
stays skipped for good, and one without a reference is not even a candidate.

Both facts are kept: `reclaimed` sits alongside `ignored` cleared, so "why is
this here again?" has an answer. Counted and said on screen, because a queue that
GREW needs a reason as much as one that shrank.

### `feat/supplies-inventory` — merged into `dev` 26 Aug at `0016d5b`

**Off:** `dev` at `b51f0cb`
**State:** merged, gate green (core clean, 190 host tests), installed
**Touches:** `sustena-core/src/operator/inventory.rs` (new), `operator/{mod,meta}.rs`,
`templates.rs`, `dto.rs`, `commands.rs`, `Orchie.tsx`

**Purpose.** A spend records money leaving. It does not record that beans,
onions and carrots arrived, and those are real things the household holds.
`inventory.itemize` takes a spend that has already landed and records what it
turned into.

**The design decision worth keeping.** Itemizing moves **no money**. The spend
already took it out of the account; if this took it again the same shilling
would leave twice. Net worth is unchanged at the moment of buying, because cash
became beans — the expense is later, when they are used up. That is `consume`,
designed for and deliberately not built.

`inventory` is a state dimension, itemize is an operator through the gate, the
log is append-only, and asset ids are derived from the purchase rather than
minted — so a replay lands byte-identical and this embroiders into a Sustain
later without rework.

**Refusals, all real:** a list that exceeds the spend (value from nowhere); the
same purchase itemized twice (doubles the shopping while the spend stands); a
line with no name or no amount, named by position; supplies with no purchase
behind them. A list *under* the spend is allowed and reports what stays a plain
spend, because a receipt half remembered is still worth recording.

**No migration.** Households opened before this dimension existed create the
list on the way in. Verified with a test that strips the key first, which is
exactly how his state is on disk.

**Deferred by the timebox:** consumption, and subpockets are stored per line and
grouped in the view but have no dedicated picker yet.

### `fix/compact-pocket-chips` — merged into `dev` 26 Aug at `8058b02`

**Off:** `dev` at `31a797e`
**State:** merged, verified on the device

Bonnie said the pocket pills were still too large, twice. They had already
become 34px chips; that was still too big for fifteen of them, because what
costs him is reading the list rather than hitting the target. Now 28px at 12px,
with long names trimmed instead of setting the row width.

The change that matters more: compactness is decided by **how many options there
are**, not by whether the field is called `pocket_name`. The old check failed
silently and open — any other long list got full-size buttons, and a rename
would have landed straight back in the wall of buttons.

**Verified on his phone over CDP**, not asserted: chips render at 28px, and
eight pockets fit in two rows of a 393px container. Previously four rows.

### `feat/skip-all-like-this` — merged into `dev` 26 Aug at `f065725`

**Off:** `dev` at `8058b02`
**State:** merged, gate green (core clean, 187 host tests)
**Touches:** `ingest.rs`, `world.rs`, `commands.rs`, `dto.rs`, `lib.rs`, `Orchie.tsx`

Design doc §5 item 8, outstanding since 25 Aug. One answer clears a whole shape
instead of the same decision made hundreds of times.

Keyed on source plus parser name — the shape the transducer recognised — never
on raw text, which never repeats exactly. An unparsed message can never teach
one: `parser_name` is empty there, so the rule would mean "skip everything I
cannot read", which is the pile that most needs his eyes. Retroactive, because
he answers this in the middle of the pile it is meant to clear.

**New file on disk:** `skip_rules.json` in the ingest directory. No schema
change, so nothing to migrate.


### `feat/shared-ref-join` — merged into `dev` 26 Aug at `31a797e`

**Off:** `dev` at `4ef4168`
**State:** merged, gate green at merge (core clean, 179 host tests)
**Touches:** `sustena-core/assets/parse_rules_seed.json`,
`apps/mycelium/src-tauri/src/{ingest.rs, world.rs, commands.rs}`

**Purpose.** Bonnie's insight, 26 Aug: the two halves of a self-transfer quote
the *same reference code*. KCB's "SEND TO M-PESA" prints `M-PESA REF:
UHPB9480T9`; the M-Pesa text confirming the same money opens with that identical
code. That is a join key, and it is a better one than amount plus time.

The distinction is not degree. Amount and merchant ask whether two texts *could*
be the same movement, and have to refuse whenever more than one candidate fits.
A shared reference says they *are*. Two identical KES 2,000 transfers to the same
person on the same day are hopeless for the first question and trivial for the
second.

**What it changed, in three places:**

1. **Transfer detection** joins on the reference first, falling back to amount
   and account only when neither text quoted one.
2. **Reversal matching** does the same, which retires the ambiguity that used to
   force a refusal whenever two identical charges sat in the queue together.
3. **The cross-source double count**, which had been deferred since 1 Aug
   explicitly for want of a reliable correlation key. This is that key. Two
   banks writing about one payment hash differently, so the raw-text dedup lets
   both in — correctly, they are two real messages — and applying both counted
   the money twice. `same_event_as` records which got there first; the second
   keeps its text and stops asking.

**Merge notes.** Added `same_event_as` to `IngestedMessage`,
`#[serde(default)]`, so records written before it read back fine. Extended
`kcb_send_to_mpesa`'s pattern with an *optional* ref group, so the earlier
example without one still matches — asserted by the rule carrying both. It was
the only branch touching `ingest.rs`, so the merge was clean.

### `feat/orchie-imc` — P1 to P4 (merged into `dev`, 26 Aug)

Capture, the phone as a Sustain, the trend series, and drawing salience. See
`docs/ORCHIE_IMC_PLAN.md`, which records what each step turned up as well as
what it built.

### Earlier, all fully merged into `dev`

Confirmed with `git branch --merged dev`: none of these hold anything `dev` does
not, so the local copies were deleted. Named here because a branch that existed
and did something is worth being able to look up.

| Branch | What it was for |
|---|---|
| `fix/orchie-android-label` | Renamed the Android app to Orchie, keeping the applicationId so it installed as an update |
| `feat/orchie-face-toggle` | Made the cockpit's Ingest screen reachable from the phone |
| `feat/sms-autoreader` | The on-device SMS reader: receiver, filters, local queue |
| `fix/sms-bulk-freeze` | The freeze on a large inbox — a card per message, against the attention budget |

### Accounts and reversal netting (landed directly on `dev`, 25–26 Aug)

Cases 1 to 3 of reversal netting, the accounts model, reconciliation against the
balance each text reports, and internal-transfer detection. See
`Projects/IO/design/Orchie_Onboarding_UX.md` §5c and §5d.

---

## Live — build resumed on the settled money model (26 Aug)

Halt lifted; building to `Orchie_Money_Model.md` §8. Order: double entry →
settled parser signs → person accounts → realignment → operative DAGs →
inventory net worth.

### `feat/double-entry` — merged into `dev` 26 Aug, installed

**Step 1 of §8.** Every transaction is postings that sum to zero, and the check
is now a **gate refusal** rather than a test.

What is checked is not "do the declared postings sum to zero" — a movement is
two-sided by construction, so that proves nothing. It is: **does what actually
changed match what was declared?**

**Measuring before enforcing found six single-sided entries**, which is the
whole argument for measuring first: `record_income`, `spend`, `unspend` and
`unrecord_income` declared the ENVELOPE (liquid or a pocket) while the money
really moved in a cash account; `inventory.consume` lowered an asset and
declared nothing; `place_unaccounted` credited an account out of nowhere.

**Two modelling points the measurement forced**, both of which would have been
wrong if assumed:

- **Liquid and pockets are the envelope view** of money a cash account already
  holds. Balancing them in the same ledger would count every shilling twice and
  make allocating to a pocket look like acquiring money.
- **`income.monthly_total` is a tally, not a balance.** Treating it as an
  account would make one arrival look like two. The income side of the entry is
  the outside party it came from.

An endpoint outside the household is the counterpart that makes a spend
two-sided, not an imbalance.

**Next in order:** the settled parser signs (Fuliza as cash + liability, fee
subpocket; M-Shwari as an account; cash-out to a Cash account; Pochi labelled
and directional), which needs the liability and person account types the ledger
now understands.

## Architecture audit

Lives in `docs/OPERATOR_AUDIT.md`, completed 26 Aug. Short version: every state
change is an operator behind the gate and that holds without exception; nothing
composes those operators into a graph and no operative is declared as one. The
realignment plan is in that document, in dependency order.

**One live inconsistency it found, worth knowing before resuming:** vendor
memory now exists twice — the `vendors` state dimension written by
`vendor.remember`, and `history.json` written by `ingest.rs::remember`, which is
what the classify UI still reads. Nothing is corrupt; they simply disagree about
who knows what. Resolving it is step 1 of the realignment.

## Halt point, 26 Aug

Building stopped here by Bonnie's call, to finish designing the money model
before more is built on it. State at the halt:

- `dev` = `origin/dev`, `main` = `origin/main`, working tree clean.
- `git branch --no-merged dev` is empty: nothing stranded, nothing half-built.
- Core suite clean; 202 host tests pass.
- His phone is running the build installed 09:56 on 26 Aug.

## The "known flake" was a real defect — fixed 29 Aug

`wire::tests` failed intermittently under a full parallel run and passed in
isolation. Carried as a flake; it was not one. Each test did five PBKDF2 unlocks
at the OWASP floor of 600,000 iterations against a 20-second socket timeout, and
under parallel load they lost the race. Fixed by lowering the KDF cost under
`cfg(test)` while asserting `PBKDF2_FLOOR` separately, so the production cost is
still checked. **The host suite went from 250+ seconds to 4.1.** The lesson is
the label: a test inherited as flaky stops being investigated.

## Conventions

- Branch from `dev`, never from `main`.
- One idea per branch. If a branch needs a second idea to be useful, that is a
  sign the first one was not finished.
- Run the full gate before merging: `cargo test` in both `sustena-core` and
  `apps/mycelium/src-tauri`, plus `npx tsc --noEmit` and `npm run build`.
- Record the finding, not only the feature. Most of these branches turned up a
  defect that reading the code had not, and the finding is usually worth more
  than the change.
