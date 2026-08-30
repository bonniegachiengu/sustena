---
number: 2
kicker: ENZYME
title: Nothing Happens Until Something Says Yes
module: Operator
dek: A reaction that would take seventy-eight million years on its own happens forty times a second, because something said yes. That gate is the smallest unit of anything happening.
date: 2026-08-30
---

There is a chemical step inside you that, left alone in water, would take about seventy-eight million years to happen once.

It is nothing dramatic. A molecule called orotidine monophosphate — a stop on the way to the building blocks of DNA — has to give up a single carbon atom. Richard Wolfenden's group measured how fast that happens with no help at all, by running the reaction hot and extrapolating back down to body temperature, and the number came back geological. Half the molecules would be done somewhere around the time the dinosaurs finished.

In your cells it happens about forty times a second.

Nothing about the molecule changed. Nobody forced it. What changed is that a protein was present — OMP decarboxylase — which binds that exact molecule and essentially nothing else, and in binding it, makes the one step the molecule was always allowed to take suddenly cheap enough to take. The reaction was never forbidden. It was merely unreachable.

That is the subject of this article. Last time we watched a cell hold itself inside a narrow band of livable states — the inland sea, the pumps, the constant paid-for work of not dissolving. This is the other half of the same question. Given a state, how does it ever *legally* become a different one? What is the smallest unit of something happening?

Biology answered with the enzyme, and the answer has the same shape everywhere it shows up.

---

#### **The clam that closes on sugar**

Take hexokinase, the protein that starts the burning of sugar. It sits in the cytoplasm with an open cleft between two lobes, waiting.

Glucose drifts in. The cleft closes around it.

Emil Fischer's 1894 image for this was a lock and key — a substrate fits an enzyme or it doesn't, and specificity is a matter of shape. Daniel Koshland sharpened it in 1958 into what actually happens: **induced fit**. The enzyme isn't a rigid keyhole. It is loose until the right molecule arrives, and the arrival *is* what folds it into the working shape.

Now the detail that should stop you. Hexokinase's job is to move a phosphate from ATP onto glucose. Water is also a small molecule with an available oxygen, and water is everywhere — it would happily take that phosphate instead, wasting the cell's fuel for nothing. The closing motion squeezes the water out. The enzyme is built so that the shape which makes the reaction possible is the same shape that makes the *wrong* reaction impossible.

So the recognition isn't a preamble to the work. It is part of the work. Restriction enzymes make the same point starkly: EcoRI cuts DNA only where it reads the exact sequence GAATTC, and passes over the entire rest of the genome in silence. The condition and the action are one object.

---

#### **Ten in a row**

Hexokinase's product is glucose-6-phosphate. It doesn't go into a queue or a dispatcher. It goes to the next enzyme, phosphoglucose isomerase, which will only accept glucose-6-phosphate — and whose own product is exactly what the enzyme after it requires.

Glycolysis is ten of these in sequence, glucose to pyruvate, and the sequence is held together by nothing but that matching. There is no wiring. Nobody scheduled it. Each step's output happens to satisfy the next step's condition for acting, and that agreement is the entire pipeline.

Which means a metabolic pathway is not a program written over the enzymes. It is a chain that exists because the links fit, and could not exist if they didn't. Put two enzymes together whose product and substrate don't match and you have not made a broken pathway. You have made no pathway.

---

#### **Which steps can run backwards**

Most of the ten steps are reversible. Triose phosphate isomerase — so fast it is limited only by how quickly molecules can bump into it — runs cheerfully in either direction, and which way it goes on any given afternoon is decided by what is piled up on each side.

Three steps do not. Hexokinase, phosphofructokinase, and pyruvate kinase each release so much energy that they effectively only go one way. When the body needs to run sugar-making *backwards* — building glucose rather than burning it — it cannot simply reverse those three. It has to route around them with entirely different enzymes.

And here is the pattern worth carrying. Those same three irreversible steps are precisely the ones the cell regulates. Every serious control knob in glycolysis sits on a step that cannot be undone.

That is not a coincidence. Where you can walk back, you can afford to be relaxed about walking forward. Where you can't, the decision has to be made properly at the door.

---

#### **The end product tells the first enzyme to stop**

In 1956 Edward Umbarger noticed something that reorganised how everyone thought about metabolic control. Bacteria making the amino acid isoleucine have a first committed enzyme, threonine deaminase, that starts the pathway. Isoleucine — the finished product, five steps later — binds that first enzyme and shuts it down.

Not by competing for the working site. By binding somewhere else entirely, a second site on the same protein, and changing its shape from there. Jacques Monod, Jeffries Wyman and Jean-Pierre Changeux gave the mechanism its model in 1965 and its name: **allostery**, "other site." Gerhart and Pardee found the same architecture in the enzyme that starts pyrimidine synthesis, throttled by the CTP it eventually produces.

Phosphofructokinase — one of glycolysis's three one-way steps — is the household version of this. It is slowed by ATP and by citrate, both signals that the cell is already energy-rich, and it is sped up by AMP, the signal that the cell is running down.

So the real question an enzyme answers is never just *is my substrate here?* It is: my substrate is here, **and** the cell as a whole is in a condition where doing this is a good idea. Both, or nothing happens. The local condition is only one term.

---

#### **The enzyme that cannot act alone**

Hexokinase does not actually bind ATP. It binds magnesium-ATP — the metal ion is not optional, and without it the reaction does not run. Further down the same pathway, glyceraldehyde-3-phosphate dehydrogenase requires NAD⁺, a molecule your body builds out of a vitamin you have to eat.

An enzyme with its substrate present, its shape correct, and its cofactor missing does nothing at all. It waits.

And when it does fire, it spends. Hexokinase's step costs a whole ATP — the cell pays, up front, for permission to begin burning the sugar it is about to burn. Every transformation in the pathway has a price tag, and the cell's ability to act is bounded by what it can afford. This is the same bill the last article described for holding a gradient. Doing costs, exactly as holding costs.

---

#### **The enzyme that checks its own work**

DNA polymerase adds one base at a time along a growing strand. Its ability to pick the right base — just from shape and hydrogen bonding — gets it wrong somewhere around once in a hundred thousand.

That is nowhere near good enough, so the enzyme does something else. After it places a base, it checks. If the pairing is wrong, a second active site on the same protein — the 3′→5′ exonuclease that Brutlag and Kornberg characterised in 1972 — clips the mistake straight back off, and the polymerase tries again. The correction is roughly a hundred- to a thousand-fold improvement, and a further repair system afterwards takes the final error rate down into the neighbourhood of one in a billion.

Read the sequence of events, because it is unusual. There is a condition before acting. There is an action. And then there is a **second condition, checked after the action, on the result** — with an undo attached to the failure.

An enzyme that verifies its own output and reverses itself when the output is wrong. Life worked out, a very long time ago, that a precondition alone is not enough.

---

#### **The barrier is not the destination**

One last thing, and it is the one that keeps everything else honest.

An enzyme lowers the barrier to a reaction. It does not change where the reaction ends up. The equilibrium — how much product versus how much substrate the world will finally tolerate — is set by the energetics of the molecules themselves, and a catalyst has no vote in it. An enzyme speeds the forward direction and the reverse direction by exactly the same factor. It gets you to the destination faster; it does not move the destination.

So an enzyme's whole power is over *what is reachable*, and none of it is over *what is allowed*.

That is a strong constraint, and it is the right one. A cell that could dissolve its own limits by making a clever enough enzyme would not be a cell for very long.

---

None of that was a metaphor, and it doesn't need to be softened into one. The enzyme is not *like* the thing Sustena calls an Enzyme; it is the clearest real instance of one that exists — and, as it happens, the pump that opened the last article, the sodium-potassium ATPase, is itself an enzyme. The thing that holds the cell in range and the thing that moves it are the same kind of object.

In Sustena, an **Enzyme** is a **guarded, effectful, partial function on state**: a **guard** — a condition over the state — and an **effect** — a change to the state, plus the events it announces. Substrate specificity is the guard. Catalysis is the effect. And *partial* is the important word: an Enzyme is simply undefined where its guard does not hold, in the same way that an enzyme presented with the wrong molecule does not do a worse job — it does nothing.

The set of all the Enzymes **is T**, the transition model. This is the strong claim, and it is exactly as strong in the cell: a reaction with no enzyme is, on the timescales that matter, not a reaction. If state can change any other way — a stray write, a hand-edited number — then the list of moves is a fiction, and every promise built on top of it is void.

**Composition is the pathway.** Enzymes chain when one's result satisfies the next one's guard — `post(A) ⊨ guard(B)`. That is what a sub-recipe is, and what an Enzyme chain is: not a wiring diagram but a claim that the links fit. And it means an impossible chain should be caught the way an impossible pathway is caught in chemistry — by the mismatch itself, at the moment you propose it, not at two in the morning when step three refuses to fire.

**Declared inverses are the reversible steps.** An Enzyme that can be undone should say so and name its inverse, and applying that inverse should genuinely restore the prior state — not approximately walk back toward it. That is principled undo, and it is the honest form of rollback. And the biology gives the corollary for free: the moves that *cannot* be undone are the ones that most deserve a gate.

**Postconditions are proofreading.** A declared condition on the result, checked after the effect, with a refusal attached — because a precondition alone has never been enough for anything that matters.

**And the guard is never only local.** The real gate is `local_guard(s) AND invariants(result) AND permitted(actor)` — the substrate is present, *and* the whole is still in a condition where the change is acceptable, *and* the one asking is allowed to ask. Allostery, feedback inhibition, and the missing cofactor are those three terms, and a cell that checked only the first would poison itself with its own products inside a day.

Which leaves the sentence that ties this article to its sibling. **The Enzymes move state; the invariants say where state may go.** T never gets to redraw V, exactly as no enzyme gets to move an equilibrium — it can only make some of the legal moves affordable. S is the space, T is the set of moves through it, and together they are the whole substrate: one recursive object, at every scale, changing only in ways it has declared and only when something says yes.
