---
number: 4
kicker: RECORD
title: Everything That Ever Happened Is Still Arriving
module: Events and Time
dek: A star collapsed 168,000 years ago and the news arrived twice, three hours apart. The gap between when a thing happened and when you heard is the most informative part of the record.
date: 2026-08-30
---

At about 07:35 on the morning of 23 February 1987, three machines buried deep underground — one in a Japanese zinc mine, one in a salt mine under Lake Erie, one beneath a mountain in the Caucasus — each registered a brief shower of particles. Between them the whole thing lasted around thirteen seconds.

Roughly three hours later, an astronomer named Ian Shelton stepped outside at Las Campanas in Chile, looked up, and saw a star in the Large Magellanic Cloud that had not been there the night before.

It was the same event. A massive star had collapsed. And it had collapsed about 168,000 years ago.

Two things about that should stop you, and the second is the one that matters. The first is obvious: the news took 168,000 years to arrive. The second is stranger. The news arrived **twice**, in two forms, three hours apart — the neutrinos streaming out of the collapsing core the moment it gave way, the light having to fight its way outward through the star's own envelope before it could leave at all. One event. Two arrival times. And the gap between *when a thing happened* and *when you found out* was not a defect in the instruments. It was the most informative thing in the whole observation.

The last three articles built something that holds itself inside a region, moves only by declared steps, and refuses the steps that would take it out. All three quietly assumed something they never examined: that there **is** a record of what happened, in order, that can be trusted. This article is about that record — and about the fact that the world keeps it faithfully and delivers it late.

---

#### **The books nobody is allowed to erase**

When Luca Pacioli wrote down the Venetian merchants' method in 1494, he was not inventing double-entry bookkeeping. He was documenting a discipline that was already old, and the part of it worth carrying here is not the two columns.

It is that a ledger is never edited.

Find a mistake in a posted account and you do not go back and scratch out the number. You write a new entry that corrects it, dated today, and the wrong figure stays on the page forever with its correction beneath it. Every auditor since has insisted on exactly this, and the reason is not moral fussiness. A book you can go back and change is a book that cannot tell you what you believed last Tuesday — and "what did we believe last Tuesday, and why did we act on it" is the only question an audit ever really asks.

So the oldest information system humans still use daily is **append-only**. The current balance is not a fact that sits somewhere being maintained. It is what you get by adding up the entries in order.

---

#### **The tree that counts its own years**

In the early twentieth century an astronomer, of all people, turned this into a science. A. E. Douglass was looking for sunspot cycles in climate and started reading tree rings, and what he found was better than what he was looking for.

A wide ring is a good year and a narrow one is a hard year, so the sequence of widths in any one trunk is a barcode of the weather it lived through. Match the barcode at the outer end of an old beam to the barcode at the inner end of a living tree, and the two records lock together — a technique called **crossdating** — and you have a single chronology longer than either sample. Chain enough of them and you get thousands of continuous years, each ring dated to its own calendar year.

Notice what makes it work. You did not date the beam by when you found it. You dated it by *what happened to it and when*, read off the record it carries with it. Two samples that arrived in your lab in either order fuse into one history, because each carries its own timeline inside itself.

And notice the other thing, which the first article already told us about bone. A career tennis player's serving arm carries more bone than the other one, built by nothing but the sum of the swings, with no file anywhere saying *you played tennis*. The tree is the same object. The trunk **is** the folded-up total of its own weather. If you know the history you can rebuild the state, because the state was never anything but the history added up.

---

#### **Eight hundred thousand winters in a stick of ice**

At Dome C on the Antarctic plateau, the EPICA project drilled more than three kilometres down and brought up snow that fell before there were modern humans — a continuous layered record reaching back past eight hundred thousand years.

The layers are annual. The dust in them is real dust from real storms. And the bubbles trapped in the ice are not a proxy for ancient air; they are ancient air, sealed at the moment the snow closed over and still sitting there.

This is the record in its purest form: ordered, immutable, and replayable. Nobody wrote a summary of the Pleistocene climate. They wrote nothing down at all. The ice simply accumulated, layer on layer, never revised, and eight hundred thousand years later you can run your finger down it and reconstruct a state of the world nobody was there to observe.

It reaches you all at once, in a crate, in a helicopter, decades after the fact — and none of that matters, because every layer carries when it happened rather than when you read it.

---

#### **One earthquake, a hundred late witnesses**

Now the hard version, because the ice and the tree were patient and the ground is not.

An earthquake is a single event at a single instant. What a seismic network receives is dozens of separate arrivals at dozens of stations, each at a different time, in an order that has nothing to do with the order of anything — and even at one station the waves separate, the fast compressional P-wave arriving well ahead of the slower, more destructive S-wave that left at the same moment from the same place.

Out of that mess seismologists reconstruct one consistent story, and they do it by the same trick as the crossdater: every arrival is converted from *when we heard it* into *when it left*, and only then are they combined. Inge Lehmann worked out that the Earth has a solid inner core in 1936 by taking seriously a handful of faint arrivals that had no business being where they were. She was reading late, out-of-order, partial mail and reconstructing the inside of a planet from it.

But there is a second question the network has to answer, and it is the question this whole article is really about.

**Have we heard from everyone yet?**

Because at some point you must stop waiting and publish a magnitude. Wait too long and the warning is useless; the S-wave has already arrived and the shelves have already come off the walls. Publish too early and a station you had not heard from yet turns up with data that would have changed the answer. Every early-warning system in the world lives on this trade, and none of them solves it — they *declare* a policy. Some issue an alert on the first few seconds of P-wave and then revise it as more stations report. Some wait. What no one gets to do is pretend the late station does not exist.

That is the real problem, and it is not "when did it happen." It is: **when have I heard enough to close the book?**

---

#### **The message you have to send twice**

One last thing, small and unglamorous, and the whole reliability of everything rests on it.

If you send a message over a channel that can lose things, you cannot ever be certain it arrived. So you wait for an acknowledgement — but the acknowledgement can be lost too, and now the sender doesn't know whether to send again, and if it sends again the receiver may get the same message twice. There is no number of round trips that fixes this. It is not an engineering shortfall; it is a fact about lossy channels, and it has been known formally since the 1970s under the name of the Two Generals.

Which leaves exactly one honest way out, and it is not on the sending side at all. **Make the second arrival do nothing.** If receiving a fact twice leaves you in the same state as receiving it once, then the sender can retry as often as it likes and the world stays correct. You do not get delivery exactly once. You get an *effect* exactly once, which is the only thing you ever wanted.

---

None of that was a metaphor, and this is where it lands.

In Sustena an **event** is a fact: `⟨id, type, payload, t_event, t_ingest, source, causes⟩` — what happened, when it happened out there, when we heard about it, who told us, and what caused it. It is written once and never edited, exactly like a ledger entry. The **log `L` is the source of truth**, and state is not stored so much as *derived*: **`s = fold(apply, s₀, L)`**. The bone, the tree ring, the ice, the balance — the same object. State is the fold of an ordered history, which is why it can always be rebuilt and always be checked.

The two timestamps are the supernova. **`skew = t_ingest − t_event` is the normal case, not an error** — an M-Pesa message written at 14:02 and delivered to the phone at 14:40 is not a bug any more than 168,000-year-old light is a bug. The world genuinely happens before the system hears about it, and a record with only one clock on it cannot tell you which of those two moments it means.

The seismic network's question becomes a **watermark**: `W(τ)` is the claim "everything with `t_event ≤ W` has now been observed," and a window `[a, b)` — a month's budget, a week's meal plan — closes when `W ≥ b` rather than when the wall clock happens to roll over. And because a station may still report after you closed, each window must **declare** what it does with a late fact: drop it, fire again with it, or accumulate it and retract the earlier answer. What no window may do is close by hope and then quietly be wrong.

The Two Generals gives the discipline: every event carries a stable `id`, the substrate dedupes on it uniformly, and at-least-once delivery plus an idempotent apply becomes **exactly-once effect**.

And then the deep one, which the crossdater and the seismologist both already knew. If every fact carries *when it happened* rather than *when it arrived*, then the same set of facts, replayed in **any** order, reconstructs the same history. For a value that is a reading of something real — a balance, a level, a temperature — the newest reading wins regardless of which order the readings reached you, and that single rule turns out to be enough. Two phones, offline, out of sync, receiving the same messages in different orders, land on the same state without ever talking to each other. That is not a convenience. It is the property that makes a household system work on a phone in a place with no reliable network, and a live system stumbled into it the hard way — after out-of-order SMS clobbered a balance, a guard was built that keeps the reading with the later *event* time. It was built before it had a name.

Which leaves the sentence the whole layer rests on. **Time in Sustena is not duration. It is causal order** — and the record of that order is the thing every other engine stands on. The gate from the last article rides on this log. The monitor reads it. Tenet replays it. Multiparty orders it. The editing engine re-runs history through a changed rulebook. None of them can be more trustworthy than the record beneath them, which is why this layer gets built first and gets built right: **an event written without its provenance and its true time is an event that can never afterwards be reordered, deduped, or believed.**
