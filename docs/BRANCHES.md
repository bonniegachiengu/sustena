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
