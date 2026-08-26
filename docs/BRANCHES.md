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

## Conventions

- Branch from `dev`, never from `main`.
- One idea per branch. If a branch needs a second idea to be useful, that is a
  sign the first one was not finished.
- Run the full gate before merging: `cargo test` in both `sustena-core` and
  `apps/mycelium/src-tauri`, plus `npx tsc --noEmit` and `npm run build`.
- Record the finding, not only the feature. Most of these branches turned up a
  defect that reading the code had not, and the finding is usually worth more
  than the change.
