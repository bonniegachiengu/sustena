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

## Known flake

`wire::tests::a_sealed_frame_survives_the_round_trip_intact` failed once under a
full parallel run on 26 Aug and passed alone, then twice more in full runs
straight after. It does real crypto round trips and nothing in the inventory work
touches `wire`. Recorded rather than ignored: a test that fails one run in four
is worth knowing about before it fails on something that matters.

## Conventions

- Branch from `dev`, never from `main`.
- One idea per branch. If a branch needs a second idea to be useful, that is a
  sign the first one was not finished.
- Run the full gate before merging: `cargo test` in both `sustena-core` and
  `apps/mycelium/src-tauri`, plus `npx tsc --noEmit` and `npm run build`.
- Record the finding, not only the feature. Most of these branches turned up a
  defect that reading the code had not, and the finding is usually worth more
  than the change.
