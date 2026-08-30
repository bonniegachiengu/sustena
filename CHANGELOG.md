# Changelog

Every released version of Sustena, newest first. Versions are `MAJOR.MINOR.PATCH`
and the single source of truth is the `VERSION` file at the repo root — running
`scripts/release.ps1` is the only thing that should change it.

While the version starts with `0.`, the shape of things is still allowed to
move: a **minor** bump (0.1 → 0.2) is where new capability lands and where
something may change how it behaves, and a **patch** bump (0.2.0 → 0.2.1) is a
fix that does not add anything.

## v1.1.0 - 2026-08-31

- fix(fold): genesis joins, ReplaceRoot still replaces — two jobs, two mutations
- chore(core): refresh the lockfile after the merge
- fix(ui): a card is never shorter than what is in it, at any width
- feat(live): state that arrives relays a pulse — Multiparty §III
- fix(fold): open in causal order, and come up without a person
- docs: start the night status file
- fix(fold): a root is joined, not replaced — Multiparty §VI
- feat(peers): let a person give a dialled-in peer an address
- feat(peers): a peering is a relationship, not a session
- fix(peers): assert the receiver re-fold in the test that has a receiver
- fix(peers): the node being synced TO never learned it had changed
- docs(peers): log what the first live two-device run found
- feat(lore): stand up Sustena Lore, and publish the first essay
- feat(ingest): a correction teaches a shape, not a sentence
- feat(ingest): universal field shapes -- read a message nobody wrote a rule for
- docs(canon): fold the money model into three papers, add the two UI laws
- feat(criticality): tail shape, as a comparison and never as a fit -- OPV-14
- docs(peers): the five rungs -- what two-node sync actually proves
- feat(ui): the Library says what runs, and Ingest classifies in place
- docs(audit): the bidirectional canon-code audit, and the two UI rules' grounding
- feat(ui): two universal rules -- fit-to-container, and navigable-looking implies navigable
- feat(orchie): the notification the Rust rewrite dropped
- fix(royalty): revert the split to the canonical four-way 70/20/5/5

## v1.0.0 - 2026-08-30

The complete build. The WBD is closed: 264 rows done, 0 open, 0 parked, 4 declined -- every one traced to the 18 canon papers.,Sustena is Sigma = <B, S, V, T, +>: state as a fold over events, an admission gate that refuses rather than corrects, composition and roll-up over nested holons, a council of operatives that advise and never act, a simulator that cannot touch reality, human-gated egress that can never move money, and an economy in which juul is the coin and pawa is the gas.,Ships alongside it: idempotent intake keyed on the transaction rather than the wording, cross-source correlation, an immune layer that keeps a one-time code on the phone it arrived on, and both apps -- Mycelium on the desktop, Orchie on Android.

- fix(lint): clean clippy across core and host -- the release gate refused
- feat(ingest): key a capture on the transaction, not on the wording -- ING-5
- docs(wbd): the last four rows -- and three closed by reading, not building
- feat(core): the price of looking, and the shape of a peg -- OPV-8 + ADD-1
- feat(core): everything that spends, spends the same meter
- feat(core): when a model may be asked, and what happens when it may not
- feat(core): what a market needs, and what does not transfer -- ARE-8
- feat(core): discovery and settlement, both behind a stable interface
- feat(core): security falls out of the meter -- PAWA-10
- feat(core): a commons with a subsidy, and Symbionts with no authority
- feat(android): a second on-device detector that fails differently -- IMM-11
- feat(core): the network is a Sustain, and knowing who is in it
- feat(core): two lifecycles in one order, and settlement that checks -- ARE-9
- feat(core): niches, selection, and what a market may not do
- fix(pawa): the reference engine runs the ratified royalty schedule -- MYC-5
- feat(core): publish is an Enzyme, not an upload -- ARE-2 + ARE-12
- feat(core): what a trust score has to be -- ARE-4
- feat(core): two prices, because they are two different objects -- ARE-3
- feat(core): the efficiency term in the scorer -- PAWA-4 + TEN-11
- docs: the WBD closed -- 227 done, 0 open, 3 declined, 38 parked
- feat(host): the loop that runs, and a queue a person can see -- CTL-9
- fix(edit): two holes in the same fence -- EDIT-14 and EDIT-15
- fix(studio): an unsubstituted token was being posted as an owner id
- feat(core): spec imports, pinned and explicit -- DSL-12 complete
- docs(wbd): a marker for declined, so it stops reading as open
- feat(core): the four meme kinds, and the one that escapes criticism
- feat(core): an Enzyme's payload reaches the durable record
- feat(core): two surfaces, one truth -- checked by nesting
- feat(core): the default council, enumerated so Corollary 1 can be checked
- feat(core): the thread from a device to a decision
- docs(wbd): ING-5 is done in core, not yet wired in the host
- feat(core): key an intake on the fact, not on the arrival or the wording
- feat(core): detection over the log that may ask but not deny
- docs(wbd): OPV-29 was already complete -- third stale row this session
- feat(core): one duality at three scales, and depth that adds only conjuncts
- feat(core): an LLM is a proposal distribution, not a caller
- feat(core): u is a vector, and the escape from Arrow's wall is declared
- feat(core): the domain x engine matrix, and the cells that refuse — CAP-13
- feat(core): a device is a Sustain, judged from the receiving seat — ING-9
- fix(core): a retry is a new decision, and who decides depends on the act
- feat(core): what a council's agreement is actually worth — OPV-12 + OPV-26
- feat(core): probe the starting point and return the front, not a winner — OPV-15
- feat(core): the candidate score, with viability as a trajectory property — OPV-18
- feat(core): positioning as reachable room under a declared measure — OPV-20
- feat(core): a simulator that reports how far ahead it still means anything — OPV-13
- feat(core): a secret detector that fails differently from the other three — IMM-11
- feat(core): metrics and traces as folds over the log — MON-3
- feat(core): egress refuses the blind retry that ingest depends on — ING-11
- feat(core): admit against the viability kernel, not just the region — CON-10
- feat(sustain): the one recursive object, and no privileged top
- feat(ingest): ack, retry and queue depth — unknown is not a no
- feat(ingest): liveness — never guess a cadence, and unknown is an answer
- feat(ingest): the message's own clock, which the patterns were already capturing
- feat(sustain): gate what nobody is optimising, using their numbers not invented ones
- feat(controller): the step bound — improving is not the same as arriving
- feat(store): the log records the call, and the wire flake was a real bug
- feat(sustain): a domain per region, and disorder detected rather than defaulted
- feat(ingest): cross-source correlation on the fact, not the reference alone
- feat(controller): the control system as one object — M-CTL complete
- feat(core): H(s) as a comparison, and admissibility along the whole holon path
- feat(dsl): description length, and when a pattern earns a name
- feat(dsl): a friendlier surface over the one AST
- feat(dsl): Enzymes authored in Embroidery, running inside the gate
- docs(branches): record feat/dsl-spec-validator
- feat(dsl): the whole-spec validator — everything a definition names
- feat(dsl): the holon path type-checked as one program
- feat(dsl): the typed overlay — hand a person the overlay, never the definition
- docs(branches): record feat/dsl-typed-params
- feat(dsl): Def(theta) — a definition is a typed function, checked once
- chore(builds): keep collected installers out of the tree
- docs(branches): record feat/dsl-type-judgment
- feat(dsl): the type judgment — a broken rule is a broken document, not a violated law
- docs(branches): record feat/parser-signs — money model §8 complete
- feat(money): the settled parser signs — debt, savings, cash and Pochi
- docs(branches): record feat/net-worth
- feat(ledger): the household position — money out is not money gone
- docs(branches): record feat/operative-dag and its three findings
- feat(operatives): Mentor and Attache as graphs, and one source of vendor truth
- feat(dag): operatives are graphs of operator calls, checked before they run
- docs(branches): record feat/person-tab-display
- feat(orchie): a person pocket reads as a person, with both sides of the tab
- feat(tab): both sides of a person tab, read back off the log
- docs(branches): feat/person-pockets merged and verified on device
- docs(branches): record feat/person-pockets and its three findings
- feat(person-pockets): a receive from a linked number pays down that tab, not income
- feat(person-pockets): a number ties a pocket to a person, and the tab runs both ways
- feat(orchie): walk the queue, and put a message off honestly
- docs: double entry landed, and what measuring first found
- feat(ledger): double entry, enforced at the gate
- docs: the operator audit, the M-Pesa type differentiation, and the halt point
- feat(transducer): learn the real M-Pesa shapes from his own traffic
- fix(money-safety): money that came in can never be recorded as a spend
- docs: the architecture audit, and the vendor and reclassify branches
- feat(orchie): move a spend filed to the wrong pocket
- feat(vendor): who you paid, what you call it, as three operators
- fix(orchie): take "start over" off the classify card
- docs: record recoverable skips, consume, and a wire flake
- feat(inventory): using something up is the real expense
- feat(orchie): a skip a reference overturns comes back
- docs: record the supplies-inventory branch
- feat(inventory): a spend can say what it brought home
- docs: record the earlier merged branches before deleting the locals
- docs: record the chips and skip-all branches
- feat(orchie): skip all like this, so one answer clears a whole shape
- fix(orchie): pocket chips small enough to scan, and keyed on length not name
- docs: record the shared-ref branch as merged
- feat(ingest): join the two halves of a transaction by their shared reference
- docs(orchie): P1 to P4 shipped, with what each one turned up
- feat(orchie): draw the ranking so a glance carries it (P4)
- feat(orchie): give the household's own distance a memory (P3)
- feat(orchie): the phone is a Sustain the household watches (P2)
- fix(orchie): compact pocket chips, and unstick his own money moving
- feat(accounts): tell his own money moving from money going out
- feat(accounts): check our arithmetic against the bank's own word
- feat(accounts): money lives in accounts, and the two readings must agree
- fix(orchie): a refund can be answered as a refund, not filed as a spend
- feat(orchie): give back a refund of a charge already filed
- feat(orchie): cancel refunds against their charges
- revert(transducer): take the reversal auto-skip back out, pending netting
- feat(orchie): skip what is not a transaction, and a picker that scales
- fix(orchie): every message gets an armed card, by construction
- feat(orchie): turn the empty-pocket refusal into a fork
- feat(orchie): make a pocket from the classify card
- fix(bindings): regenerate after the failed export truncated the file
- feat(sms): a high-water mark, so a repeat read only looks at what is new
- fix(orchie): show the message being filed, and get the feed off the UI thread
- fix(transducer): honour the format specs the shipped rules were written with
- fix(orchie): the classify surface is a widget, not a parallel list
- fix(orchie): one attention row per tier, never one per message
- fix(sms): read and drain off the UI thread, a page at a time
- docs: the Orchie ingest, monitor and controller plan
- chore: sync sustena-core's lockfile to its manifest version
- feat(sms): the button, and the sweep that runs when the app opens
- feat(sms): host commands that drain captured texts into the engine
- chore(sms): ignore the plugin's generated android artifacts
- feat(sms): plugin crate that reads M-Pesa and KCB texts on the device
- copy: last code names out of the interface (status belt, rules label, lock)
- copy: plain language across every screen, Mycelium and Orchie
- fix(layout): stop panel cards overlapping and clipping at phone width
- fix(cockpit): size the rail for the device's text zoom, not the stylesheet's
- fix(cockpit): make the shell navigable at phone width
- feat(orchie): give the phone a way into the cockpit, and so into Ingest

## v0.3.2 - 2026-08-25

Rename the Android app's label to Orchie. Repair mojibake in the manifests and stop it recurring. Resolve the Android NDK path instead of hardcoding it.

- fix(build): repair remaining mojibake, guard the Android label against stale gen/
- fix(android): label the phone app "Orchie", repair mojibake, unbreak NDK path

## v0.3.1 - 2026-08-23

Fixes the stall after unlock: Orchie sat on its loading skeleton for ever instead of showing your household.
Also stops the loading animation cooking the phone, and makes any future stall say what went wrong instead of shimmering in silence.

- fix(orchie): never ask the engine for the Sustain called ""
- fix(devops): stage the installer for the version being released

## v0.3.0 - 2026-08-21

Orchie is a phone app now, not the cockpit shrunk to fit.
Bigger everything you tap, the keyboard finally gets out of the way, taps answer instantly, and the classifying job is the first thing on the screen.

- feat(orchie): a phone app, not a cockpit shrunk to fit
- fix(devops): read the changelog as UTF-8, and repair what was mangled
- fix(devops): write VERSION without a byte-order mark

## v0.2.0 - 2026-08-21

Working channels, one-command releases, and CI that finally covers the live Rust build.
Peer transport with quorum-gated writes and encrypted sessions; packages over the wire; a trust reading with real signals behind it; Orchie on Android; the panel-scroll and Android-launch fixes.

- fix(devops): keep the tool's output out of the exit code
- fix(devops): exit codes decide, not stderr text
- chore(devops): working channels, one version, one release command
- fix(mycelium): the debug APK aborted before its first frame; gate the bindings export to desktop
- fix(mycelium): a panel taller than the view scrolls, as a layout primitive
- feat(android): Orchie on a phone — debug APK, engine embedded
- feat(trust,orders): a reading with something behind it, and juul orders
- feat(arena): packages over the transport, with no wire-specific install
- feat(wire): authenticated, encrypted, forward-secret sessions
- chore(repo): untrack a stray SQLite journal
- feat(quorum): agreement before the append — 1000, not 700

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
