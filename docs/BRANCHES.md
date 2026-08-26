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
