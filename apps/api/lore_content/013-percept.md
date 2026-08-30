---
number: 13
kicker: PERCEPT
title: Nothing Reaches You That Did Not Outbid Something Else
module: Curated UI
dek: Half of viewers miss a gorilla walking through a screen they are staring at. It was never hidden. It lost an auction whose prize is your awareness.
date: 2026-08-30
---

In 1999, Daniel Simons and Christopher Chabris filmed two teams passing basketballs — one in white shirts, one in black — and asked viewers to count the passes made by the team in white.

Partway through the film a woman in a gorilla suit walks into the middle of the frame, turns to the camera, beats her chest, and walks off. She is on screen for about nine seconds.

Roughly half the viewers do not see her. Not "forget" and not "neglect to mention" — they watch a gorilla cross the centre of a screen they are staring at, report afterwards that nothing unusual happened, and are startled when the tape is replayed.

What should stop you is *why*. Nothing was hidden, and there was no trick of contrast or timing. Her image landed on their retinas exactly as it landed on everyone else's, and the signal travelled as far into the brain as any other signal in the frame. It simply lost the last competition — the one whose prize is awareness — because a counting task had already settled it.

That is not a failure of vision. It is what vision *is*.

---

#### **The firehose and the straw**

The numbers set the problem.

The back of each eye holds on the order of a hundred million photoreceptors. The optic nerve leaving it carries roughly one million fibres. Before any of it reaches your brain the retina has already discarded some ninety-nine percent of what it caught — not at random, but by computing differences: edges, motion, contrast, change.

And that is the generous end. In 1956 George Miller collected the experiments on how much a person can hold in mind at once and found them clustering stubbornly around seven — "the magical number seven, plus or minus two," offered half in exasperation at a number that would not stop appearing. Forty-five years later Nelson Cowan revisited the question with rehearsal and chunking controlled for, and put the honest figure at about **four**.

A hundred million in. A million down the wire. Four in mind.

None of that ratio is a defect waiting to be engineered away. It is the operating condition. Whatever the brain does with the world, it does not show it to you.

---

#### **Found before you looked**

In 1980 Anne Treisman and Garry Gelade had people search a field of items for a target. A red dot among green dots is found in about the same time whether there are five distractors or fifty — the target *pops out*, and clutter costs nothing. Ask for a red X among red Os and green Xs, and the time climbs steadily with the number of items, as if they were being examined one at a time.

Their explanation: simple features — colour, orientation, size, motion — are registered in parallel across the whole visual field before attention arrives anywhere. Combinations are not. Combinations need attention, one item at a time.

Read that as an architecture and it is startling. **The ranking happens before the looking.** Something cheap and parallel scores the whole field, and the expensive serial machinery is then aimed at the winner. Salience is not what you notice; it is what decides what you are going to notice.

---

#### **The patch you aim**

Sharp vision comes from the fovea, a patch of retina covering roughly the width of your thumbnail at arm's length. Everything outside it is periphery — low resolution, colour-poor, very good at motion and change. So the eye moves: three or four times a second it jumps, in saccades, and between jumps holds still and reads. You take in a whole scene ambiently and *look* at almost none of it, and where the sharp patch lands next is chosen by the parallel salience map, not by a decision you experience making.

The Monitor article called this watching versus looking. Here it is at the retina: everything watched, cheaply and continuously; the one expensive instrument aimed only where the cheap process says.

---

#### **A filter that knows your name**

In 1953 Colin Cherry described the cocktail-party problem — how a listener follows one voice through a room of competing ones. In 1959 Neville Moray added the part that matters here. Feed an unattended stream into the other ear and almost none of it is later reported, with one reliable exception: a person's own name gets through, for about a third of listeners.

So the filter is not deaf. It evaluates, continuously and cheaply, and a small number of things are worth an interrupt.

Donald Broadbent had given the shape of it in 1958: a limited-capacity channel with a selective filter in front. Fifteen years later Daniel Kahneman sharpened it in *Attention and Effort* — attention is not a gate that is open or shut but a pool of capacity, allocated by policy, and *spent*.

Which is the sentence everything turns on. If attention is spent, then everything that reaches you was bought — and something else was not.

---

None of that was a metaphor.

Sustena's Curated UI is the orchestrator's rendering surface, and it is built on exactly this arrangement.

A widget is **declared**, not drawn: `w = ⟨inputs, render, emits⟩` — the state it may read, how it draws, the Enzymes it may offer. And it is type-checked like every other declaration in the system, which is the Genome article's rule arriving at the screen: `inputs` must name dimensions that exist, `emits` must name Enzymes that exist. A button labelled "View Budget" is a string, and means whatever its handler happens to do. A typed `emits` entry is an Enzyme, agreed before anything reads it.

A view is not a page. It is `compose(r)` — a value computed per request from `r = ⟨state, query, device⟩`, never stored, never navigated to. The screen is an answer to a question asked now.

And composition starts from **what happened**. A binding table takes an event class and returns the widgets that serve it, so the person is no longer the one supplying coordinates — group, then container, then entity, held in working memory while they try to do something else. The log already has them. That inversion is the whole difference between a surface that asks you to remember your own schema and one that hands you a decision.

Candidates are then scored. Salience is urgency plus relevance, and urgency is not a new invention: it is the Monitor's own distance from the viable region — how far this Sustain has travelled toward a wall it declared for itself. There is deliberately **no second notion of important**, and the reason is not tidiness. If the screen kept a private measure, "important" would quietly become *whatever gets you on screen*, and every widget would be written to win it. Urgency being a fact about the household is what makes it un-gameable. Relevance is scent — the cheap cue that this is the thing you were reaching for.

Then selection, and this is the part that earns the article. Candidates are not sorted and truncated. They go into a **0/1 knapsack under a budget of about four**, and the solver returns what was chosen *and* what was displaced, each with its score. A widget's presence is therefore not a designer's assertion. It is a result: *this one earned its slot by outscoring that one.* "Everything on this screen is necessary" stops being a slogan and becomes a receipt you can ask to see.

Effect-first capture is the same inversion pointed the other way. A person narrates an **effect** — "sold two pilau", "that 280 was groceries" — and the engine infers which Enzyme and which parameters would have produced it, recovering the coordinates from the household's own live state rather than asking anyone to type a pocket name. Then it routes the result through the same gate as every other write. The Law article's rule holds without exception at the screen: a proposal is never a write, and the confirmation is the only door.

And the shape underneath is the one Sustena keeps finding: a scarce resource, a threshold, and the discipline to surface only what crossed it. The Monitor spends attention while watching. Pawa spends compute. The gate spends admittance. This module spends the four things a person can hold at once — the same recursive Sustain, in the coat it wears when somebody is looking at it.

The gorilla experiment hands you the closing sentence for free. Awareness is not a window onto what is there; it is a budget, and everything inside it displaced something that wanted the slot. **A screen is not a place you go. It is what won.**

---


---

#### **Two ways a screen can lie to you**

Everything above is about what wins a place on your screen. There are two ways to win that place and still deliver nothing, and both of them cost you more than the space they took.

The first is simple: the thing does not fit. It won its slot, and then it painted past the edge of the box that was holding it, and the part you needed went under the status bar. You will not notice. Nothing looks missing — a screen with a piece cut off looks exactly like a screen. That is the whole problem: you cannot go looking for something that gives you no sign it exists.

The second is subtler and does more damage. Something *looks* like it leads somewhere. A row of a list, a name, a number that seems to want tapping. You tap it. Nothing happens.

The cost is not the tap. The cost is that you have just learned something about this app, and what you learned is wrong: that rows like that one are not worth trying. A handful of dead ends and you stop trying the ones that work. **One row that lies quietly poisons every honest row that resembles it** — and the app becomes a maze, not because anything was hidden, but because you were taught to stop looking.

Which is why the rule runs both ways. If it looks like a door it must open. And if there is genuinely nowhere to go, it has to look like what it is — a fact, sitting there, not inviting anything.
