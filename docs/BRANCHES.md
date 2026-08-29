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
