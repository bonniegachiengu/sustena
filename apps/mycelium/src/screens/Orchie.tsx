/**
 * ORCHIE — the phone-first curated face.
 *
 * Mycelium shows you a household. Orchie shows you **the few things that need
 * you**, one at a time, and nothing else. Same app, same engine, same gate, same
 * login — a different question.
 *
 * ★★★ **Nothing here ranks anything.** `compose(r)` in the engine binds widgets
 * to what happened, scores them `α·urgency + λ·relevance` under the household's
 * declared policy, and picks what fits the attention budget with a 0/1 knapsack.
 * This file renders what came back, in the order it came back.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * ★★★ **WHAT THIS REWRITE IS FOR.** The verdict on the first version was
 * "clunky and impossible to use, slow, and the keyboard isn't wired". All three
 * were true and none of them were the engine:
 *
 *   • It was drawn at the COCKPIT'S scale — 10-11px text, ~22px tap targets —
 *     on a phone, for people who are not looking for an excuse to squint.
 *   • Every tap waited on a round trip with nothing but a disabled button to
 *     show for it, and every refresh replaced the whole screen with the words
 *     "reading the household…", so the layout was thrown away and rebuilt.
 *   • Nothing knew the keyboard existed.
 *
 * So: Orchie's own scale (`ui/orchie.css.ts`), the keyboard wired for real
 * (`lib/viewport.ts`), and the classify flow promoted to the top of the screen
 * and opened on arrival — because THAT is the thing his mum actually does.
 *
 * ★★★ **The classifying job comes first, always.** Not because it ranks highest
 * — the engine decides ranking and this file does not argue — but because a
 * captured message is the one card that is a JOB rather than a READING.
 * Everything else on this screen is something to know; this is something to do.
 * ─────────────────────────────────────────────────────────────────────────────
 *
 * ★★★ **Every card can still say why it is here** — its real urgency, whether
 * that urgency was *measured*, its relevance, its score. And what stayed quiet
 * still says which kind of quiet: **withdrawn** (nothing to say, carries a
 * reason) or **outranked** (considered, carries a score).
 *
 * ★★ The capture flow asks **one question at a time**, with options that are the
 * household's own pocket names, and stops at a confirmation every time. A
 * history pre-fill saves the tap, never the confirm.
 */
import { createResource, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import {
  engine,
  fmt,
  type FeedDto,
  type GateResult,
  type InferenceDto,
  type JsonValue,
} from "../lib/engine";
import { world } from "../lib/live";
import { keyboardAware, watchViewport } from "../lib/viewport";
import * as O from "../ui/orchie.css";

/** ★ The projection is a JSON value on the wire; read it by name, once. */
const read = (feed: FeedDto, key: string): JsonValue | undefined => {
  const r = feed.reading;
  return r && typeof r === "object" && !Array.isArray(r)
    ? (r as Record<string, JsonValue>)[key]
    : undefined;
};
const num = (feed: FeedDto, key: string): number => Number(read(feed, key) ?? 0);
const str = (feed: FeedDto, key: string): string => String(read(feed, key) ?? "—");

/** What each declared card is called, in words. ★ One place. */
const TITLE: Record<string, string> = {
  pocket_strain: "a pocket is under strain",
  classify_capture: "something needs classifying",
  household_summary: "the household",
  recent_spend: "money went out",
  recent_income: "money came in",
};

/**
 * ★★ `onFace` is how a phone leaves Orchie.
 *
 * Optional, because Orchie must still render on its own; when the shell passes
 * it, the header grows the one control that makes the cockpit reachable from a
 * touch device. See `faceToggle` in orchie.css.ts for why it exists at all.
 */
export default function Orchie(props: { onFace?: () => void }) {
  /**
   * ★★★ **`null`, never `""`. This one line was the whole bug.**
   *
   * `createResource` skips its fetcher only for `false | null | undefined`. An
   * empty string is a perfectly ordinary value, so `?? ""` did not mean "no
   * selection yet" to Solid -- it meant "go and fetch the Sustain whose id is
   * the empty string". The engine answered exactly as it should:
   *
   *     Error: no Sustain called ''
   *
   * and the resource wedged on that first rejection. On the phone the screen
   * then sat on its loading skeleton for ever, on real data, with no error
   * shown, because the failed fetch was for an id nobody had asked about.
   *
   * ★★ It only ever bit on a phone, which is why it survived every check. The
   * unlocked tree renders the moment the identity opens, while `refreshWorld`
   * is still in flight -- so there is a real window where nothing is selected
   * yet. On a laptop the face is Mycelium and Orchie is not mounted during that
   * window; on a phone Orchie IS the face, so it mounts straight into it, every
   * single time.
   */
  const sustain = () => world.selected;
  const [feed, { refetch }] = createResource(sustain, (id) => engine.feed(id, null));
  const [showQuiet, setShowQuiet] = createSignal(false);

  onMount(watchViewport);

  /**
   * ★★ Has the first load been going on unreasonably long?
   *
   * Only ever used to add a line of explanation to the skeleton. It changes
   * nothing about what is fetched and it never hides anything -- it exists
   * because a loading state with no upper bound is indistinguishable from a
   * broken one, and that is exactly how the empty-string stall stayed hidden.
   */
  const [slow, setSlow] = createSignal(false);
  onMount(() => {
    const t = window.setTimeout(() => setSlow(true), 10_000);
    onCleanup(() => window.clearTimeout(t));
  });

  // ★★★ `feed.latest` rather than `feed()`. During a refetch, `feed()` is
  //     undefined and the whole screen would fall back to a placeholder — which
  //     is what made every confirmation feel like a stall. `latest` holds the
  //     previous answer until the new one lands, so the layout never collapses;
  //     it dims (see `staleWhileRefreshing`) and the numbers change in place.
  const shown = () => feed.latest;

  /** The messages that need a person. ★ This is the job, and it goes first. */
  const jobs = () => (shown()?.attention ?? []).filter((a) => a.messageId);
  /** Everything else that needs attention but is not a classifiable capture. */
  const notices = () => (shown()?.attention ?? []).filter((a) => !a.messageId);

  return (
    <div class={O.frame} data-face="orchie">
      <div class={O.column}>
        <div class={O.header}>
          <span class={O.brand}>ORCHIE</span>
          <span class={O.headerMeta}>{shown()?.label ?? ""}</span>
          {/* ★★ The cockpit, which is where Ingest lives -- the source picker
              and the paste field for a real M-Pesa or KCB message. Reachable
              from the phone the messages arrive on, which it was not. */}
          <Show when={props.onFace}>
            {(go) => (
              <button class={O.faceToggle} onClick={() => go()()}>
                mycelium
              </button>
            )}
          </Show>
        </div>

        {/* ★★★ **Every way of having nothing to show now says which one it is.**
            The stall that shipped was invisible precisely because "loading" and
            "wedged" looked identical -- an endless shimmer with no way to tell
            them apart, on either side of the screen. Four distinct answers now,
            and the last one exists so that a stall can never be silent again. */}

        {/* the world itself could not be read */}
        <Show when={world.error && !shown()}>
          <div class={O.errorBox}>
            the household could not be opened — {world.error}
            <br />
            nothing is shown rather than something stale.
          </div>
        </Show>

        {/* the world opened, and there is genuinely nothing in it */}
        <Show when={!world.error && world.loaded && !world.selected}>
          <p class={O.empty}>
            no household on this device yet
            <br />
            nothing to show until there is one
          </p>
        </Show>

        {/* the feed itself refused or failed */}
        <Show when={feed.error && !shown()}>
          <div class={O.errorBox}>
            the household could not be read — {String(feed.error)}
            <br />
            nothing is shown rather than something stale.
          </div>
        </Show>

        {/* genuinely still loading */}
        <Show when={!shown() && !feed.error && !world.error && !(world.loaded && !world.selected)}>
          <FeedSkeleton />
          {/* ★★ A shimmer that never ends is a lie of omission. After ten
              seconds this stops pretending it is nearly there. */}
          <Show when={slow()}>
            <p class={O.caption}>
              this is taking longer than it should — the household is not
              answering
            </p>
          </Show>
        </Show>

        <Show when={shown()}>
          {(f) => (
            <div
              class={O.stack}
              classList={{ [O.staleWhileRefreshing]: feed.loading }}
            >
              {/* ═══ THE JOB — first, open, and the only amber card ═══════ */}
              <For each={jobs()}>
                {(a) => (
                  <div class={O.cardPrimary}>
                    <div class={O.cardHead}>
                      <h2 class={O.cardTitle}>{a.what}</h2>
                    </div>
                    <p class={O.caption}>{a.why}</p>
                    <Classify
                      sustain={f().sustainId}
                      messageId={a.messageId!}
                      autoStart
                      onDone={() => void refetch()}
                    />
                  </div>
                )}
              </For>

              {/* ═══ the calm read ═══════════════════════════════════════ */}
              <Summary feed={f()} />

              {/* ═══ notices that are not a classifiable capture ═════════ */}
              <Show when={notices().length > 0}>
                <div class={O.card}>
                  <h2 class={O.cardTitle}>needs you</h2>
                  <For each={notices()}>
                    {(a) => (
                      <div class={O.stack}>
                        <div class={O.row}>
                          <span class={O.badge[a.severity === "danger" ? "danger" : "warn"]}>
                            {a.severity === "danger" ? "urgent" : "soon"}
                          </span>
                          <span class={O.body}>{a.what}</span>
                        </div>
                        <p class={O.caption}>{a.why}</p>
                      </div>
                    )}
                  </For>
                </div>
              </Show>

              {/* ═══ the ranked feed ═════════════════════════════════════ */}
              <Show
                when={f().cards.length > 0}
                fallback={
                  <Show when={jobs().length === 0}>
                    <p class={O.empty}>nothing needs you right now</p>
                  </Show>
                }
              >
                <For each={f().cards}>
                  {(c) => (
                    <div class={O.card}>
                      <div class={O.cardHead}>
                        <h2 class={O.cardTitle}>{TITLE[c.id] ?? c.id}</h2>
                        <span class={O.spacer} />
                        <Show when={c.urgency > 0}>
                          <span class={O.badge[c.urgency > 0.5 ? "danger" : "warn"]}>
                            {Math.round(c.urgency * 100)}%
                          </span>
                        </Show>
                      </div>
                      <CardBody card={c} feed={f()} />
                      <Why card={c} />
                    </div>
                  )}
                </For>
              </Show>

              {/* ═══ narrate anything ════════════════════════════════════ */}
              <div class={O.card}>
                <h2 class={O.cardTitle}>something else happened?</h2>
                <Classify sustain={f().sustainId} messageId={null} onDone={() => void refetch()} />
              </div>

              {/* ═══ what stayed quiet ═══════════════════════════════════ */}
              <Show when={f().quiet.length > 0}>
                <button class={O.linkish} onClick={() => setShowQuiet((q) => !q)}>
                  {showQuiet() ? "▾" : "▸"} {f().quiet.length} other{" "}
                  {f().quiet.length === 1 ? "thing" : "things"} stayed quiet
                </button>
                <Show when={showQuiet()}>
                  <div class={O.card}>
                    <For each={f().quiet}>
                      {(q) => (
                        <div class={O.stack}>
                          <div class={O.row}>
                            <span class={O.body}>{TITLE[q.id] ?? q.id}</span>
                            <span class={O.spacer} />
                            {/* ★★ Withdrawn and outranked are different facts. */}
                            <span class={O.badge.quiet}>
                              {q.withdrew ? "nothing to say" : `outranked · ${q.score?.toFixed(2)}`}
                            </span>
                          </div>
                          <Show when={q.reason}>
                            <p class={O.caption}>{q.reason}</p>
                          </Show>
                        </div>
                      )}
                    </For>
                    <p class={O.caption}>
                      A card that <strong>withdrew</strong> had nothing to say and carries a reason;
                      one that was <strong>outranked</strong> was considered and carries a score.{" "}
                      {f().spent} of {f().budget} attention spent across {f().candidatesConsidered}{" "}
                      candidates.
                    </p>
                  </div>
                </Show>
              </Show>
            </div>
          )}
        </Show>
      </div>
    </div>
  );
}

/**
 * ★★★ The first-load placeholder, in the SHAPE of the answer.
 *
 * Not a spinner. A spinner says "wait" and nothing else; a skeleton says "a
 * heading, then a number, then two cards" — so the eye is already where the
 * content will be, and the arrival is a fill rather than a jump.
 */
function FeedSkeleton() {
  return (
    <div class={O.stack}>
      <div class={O.card}>
        <div class={O.skelLine.title} />
        <div class={O.skelLine.figure} />
        <div class={O.skelLine.short} />
      </div>
      <div class={O.card}>
        <div class={O.skelLine.title} />
        <div class={O.skelLine.text} />
        <div class={O.skelLine.short} />
      </div>
    </div>
  );
}

/** The calm read: one plain figure, never the machinery. */
function Summary(props: { feed: FeedDto }) {
  const total = () =>
    props.feed.rollup?.aggregates.find((a) => a.childPath === "finances.liquid.balance");
  const members = () => total()?.included.filter((c) => !c.isHousehold).length ?? 0;

  return (
    <div class={O.card}>
      <span class={O.figureLabel}>
        {total()?.grounded ? "everything, together" : "liquid"}
      </span>
      <span class={O.figure}>
        {total()?.grounded ? fmt(total()?.value ?? null) : fmt(props.feed.liquid)}
      </span>
      <Show when={total()?.grounded}>
        <p class={O.caption}>
          folded across {members()} member{members() === 1 ? "" : "s"} and this household's own,
          fresh — nothing stored
        </p>
        {/* ★★★ An exclusion is never silent. A figure that quietly counted an
            unreadable member as zero would read as calm and be wrong. */}
        <Show when={(total()?.excluded.length ?? 0) > 0}>
          <p class={O.caption}>
            {total()?.excluded.length} left out, not counted as zero:{" "}
            {total()
              ?.excluded.map((e) => e.label)
              .join(", ")}
          </p>
        </Show>
      </Show>
    </div>
  );
}

/** ★★★ "Why am I seeing this?" — real numbers, never a rationalisation. */
function Why(props: {
  card: {
    urgency: number;
    measured: boolean;
    basis: string;
    relevance: number;
    score: number;
    cost: number;
    eligibility: string;
  };
}) {
  const [open, setOpen] = createSignal(false);
  return (
    <>
      <button class={O.linkish} onClick={() => setOpen((o) => !o)}>
        {open() ? "▾" : "▸"} why am I seeing this?
      </button>
      <Show when={open()}>
        <div class={O.stack}>
          <p class={O.caption}>{props.card.eligibility}</p>
          {/* ★★ A 0 on an undeclared basis is silence, not safety. */}
          <p class={O.caption}>
            {props.card.measured
              ? `urgency ${props.card.urgency.toFixed(2)} · ${props.card.basis}`
              : `urgency not measured — ${props.card.basis}`}
          </p>
          <p class={O.caption}>
            relevance {props.card.relevance.toFixed(2)} · score {props.card.score.toFixed(2)} ·
            costs {props.card.cost} of the attention budget
          </p>
        </div>
      </Show>
    </>
  );
}

/** What a card draws. ★ Keyed off its declared `render` tag. */
function CardBody(props: { card: { id: string; render: string }; feed: FeedDto }) {
  const fraction = () => {
    const allocated = num(props.feed, "worst_allocated");
    return allocated > 0 ? num(props.feed, "worst_spent") / allocated : null;
  };

  return (
    <Show
      when={props.card.render === "pocket_strain"}
      fallback={<PlainCard feed={props.feed} card={props.card} />}
    >
      <span class={O.figureLabel}>{str(props.feed, "worst_pocket")}</span>
      <span class={O.figure}>{fmt(num(props.feed, "worst_spent"))}</span>
      <p class={O.caption}>of {fmt(num(props.feed, "worst_allocated"))} set aside</p>
      <div class={O.meter}>
        <div
          class={O.meterFill}
          style={{
            width: `${Math.min(100, (fraction() ?? 0) * 100)}%`,
            background: (fraction() ?? 0) > 0.9 ? "#e05050" : "#E8A020",
          }}
        />
      </div>
      <p class={O.caption}>the limit is the one you set for it</p>
    </Show>
  );
}

function PlainCard(props: { feed: FeedDto; card: { id: string } }) {
  return (
    <Show
      when={props.card.id === "classify_capture"}
      fallback={
        <>
          <span class={O.figureLabel}>liquid</span>
          <span class={O.figure}>{fmt(num(props.feed, "liquid"))}</span>
        </>
      }
    >
      <span class={O.figureLabel}>waiting on you</span>
      <span class={O.figure}>{str(props.feed, "unclassified")}</span>
      <p class={O.caption}>each one needs a pocket before it can be recorded</p>
    </Show>
  );
}

/**
 * ★★★ The signature flow: an effect → `infer` → **one question at a time** →
 * confirm → the operator through the real gate.
 *
 * ★★ `autoStart` matters more than it looks. Before, a captured message showed
 * a "classify this" button whose only job was to trigger the inference the
 * screen was always going to need — a tap, and a wait, that bought nothing. On
 * a card that exists *because* something needs classifying, the first question
 * should already be on screen when you get there.
 */
function Classify(props: {
  sustain: string;
  messageId: string | null;
  autoStart?: boolean;
  onDone: () => void;
}) {
  const [text, setText] = createSignal("");
  const [known, setKnown] = createSignal<Record<string, JsonValue>>({});
  const [ignoreHistory, setIgnoreHistory] = createSignal(false);
  const [step, setStep] = createSignal<InferenceDto | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [verdict, setVerdict] = createSignal<GateResult | null>(null);
  const [learned, setLearned] = createSignal<string | null>(null);
  // ★★ The params a person actually confirmed. Learning synthesises a rule from
  //    THESE — the confirmed amount has to be findable in the message text, so
  //    sending an empty object would teach nothing and say it learned.
  const [confirmed, setConfirmed] = createSignal<JsonValue | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);
  // ★★★ Optimistic: the option a thumb just landed on, held lit while the
  //     engine answers. Without this a tap produced NOTHING visible until the
  //     round trip returned, which is the single biggest reason it felt dead.
  const [pending, setPending] = createSignal<string | null>(null);
  // How many questions have been answered — a short trail, so the flow has a
  // visible bottom to it rather than feeling like it could go on forever.
  const [answered, setAnswered] = createSignal(0);

  const run = async (next: Record<string, JsonValue>) => {
    setFailure(null);
    setBusy(true);
    try {
      setKnown(next);
      setStep(
        await engine.infer(
          props.sustain,
          props.messageId,
          props.messageId ? null : text() || null,
          next as JsonValue,
          ignoreHistory(),
        ),
      );
    } catch (e) {
      setFailure(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
      setPending(null);
    }
  };

  const answer = (field: string, value: string) => {
    setPending(value);
    setAnswered((n) => n + 1);
    void run({ ...known(), [field]: value });
  };

  const reset = () => {
    setStep(null);
    setKnown({});
    setAnswered(0);
    setIgnoreHistory(false);
    setPending(null);
  };

  const confirm = async (s: Extract<InferenceDto, { status: "ready" }>) => {
    setBusy(true);
    setFailure(null);
    try {
      const v = await engine.confirm(
        props.sustain,
        s.operator,
        s.params,
        props.messageId,
        s.description,
      );
      setVerdict(v);
      setConfirmed(s.params as JsonValue);
      if (v.verdict === "admitted") {
        reset();
        setText("");
        props.onDone();
      }
    } catch (e) {
      setFailure(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  const remember = async (operator: string, params: JsonValue) => {
    if (!props.messageId) return;
    try {
      setLearned(await engine.learnRule(props.messageId, operator, params));
    } catch (e) {
      setFailure(String(e).replace(/^Error:\s*/, ""));
    }
  };

  // ★ A card that exists because something needs classifying opens already
  //   asking. One fewer tap, and one fewer wait, on the most common path.
  onMount(() => {
    if (props.autoStart && props.messageId) void run({});
  });

  return (
    <div class={O.stack}>
      {/* ── narration entry (only when there is no captured message) ───── */}
      <Show when={!props.messageId && !step() && !verdict()}>
        <input
          ref={keyboardAware}
          class={O.input}
          type="text"
          enterkeyhint="go"
          autocapitalize="none"
          autocomplete="off"
          placeholder="spent 500 on food"
          value={text()}
          onInput={(e) => setText(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.currentTarget.blur();
              void run({});
            }
          }}
        />
        <button
          class={`${O.action.primary} ${O.actionWide}`}
          onClick={() => void run({})}
          disabled={busy() || text().trim() === ""}
        >
          <Show when={busy()} fallback="work it out">
            <span class={O.working} /> working it out…
          </Show>
        </button>
      </Show>

      {/* ── a captured message that is not auto-started ────────────────── */}
      <Show when={props.messageId && !props.autoStart && !step() && !verdict()}>
        <button
          class={`${O.action.primary} ${O.actionWide}`}
          onClick={() => void run({})}
          disabled={busy()}
        >
          <Show when={busy()} fallback="classify this">
            <span class={O.working} /> working it out…
          </Show>
        </button>
      </Show>

      {/* ── the first inference on an auto-started card ────────────────── */}
      <Show when={props.autoStart && !step() && busy() && !verdict()}>
        <div class={O.stack}>
          <div class={O.skelLine.text} />
          <div class={O.options}>
            <div class={O.skeleton} style={{ height: "56px" }} />
            <div class={O.skeleton} style={{ height: "56px" }} />
          </div>
        </div>
      </Show>

      <Show when={step()}>
        {(s) => (
          <div class={O.stack}>
            {/* ═══ one question at a time ═══════════════════════════════ */}
            <Show when={s().status === "needsDisambiguation"}>
              {(() => {
                const q = () => s() as Extract<InferenceDto, { status: "needsDisambiguation" }>;
                return (
                  <div class={O.stack}>
                    <Show when={answered() > 0}>
                      <div class={O.steps}>
                        <For each={Array.from({ length: answered() })}>
                          {() => <span class={O.stepDot.done} />}
                        </For>
                        <span class={O.stepDot.todo} />
                      </div>
                    </Show>

                    <h3 class={O.cardTitle}>{q().question}</h3>
                    <Show when={q().why}>
                      <p class={O.caption}>{q().why}</p>
                    </Show>

                    {/* ★ options: null means the answer is not a tap. */}
                    <Show
                      when={q().options}
                      fallback={
                        <>
                          <input
                            ref={keyboardAware}
                            class={O.inputNumeric}
                            // ★★ `decimal` gives a numeric pad WITH a decimal
                            //    separator; `numeric` gives digits only, which
                            //    makes 1500.50 impossible to type.
                            inputmode="decimal"
                            enterkeyhint="done"
                            placeholder="0"
                            onKeyDown={(e) => {
                              if (e.key === "Enter") {
                                const v = e.currentTarget.value;
                                e.currentTarget.blur();
                                answer(q().field, v);
                              }
                            }}
                          />
                          <p class={O.caption}>type the amount, then press done</p>
                        </>
                      }
                    >
                      {(opts) => (
                        <div class={O.options}>
                          <For each={opts()}>
                            {(o) => (
                              <button
                                class={pending() === o.value ? O.optionChosen : O.option}
                                disabled={busy()}
                                onClick={() => answer(q().field, o.value)}
                              >
                                {o.label}
                              </button>
                            )}
                          </For>
                        </div>
                      )}
                    </Show>

                    <button class={O.linkish} onClick={reset}>
                      start over
                    </button>
                  </div>
                );
              })()}
            </Show>

            {/* ═══ ready to record ══════════════════════════════════════ */}
            <Show when={s().status === "ready"}>
              {(() => {
                const rd = () => s() as Extract<InferenceDto, { status: "ready" }>;
                return (
                  <div class={O.stack}>
                    <p class={O.body}>{rd().why}</p>

                    {/* ★★ A history pre-fill is never silent. */}
                    <Show when={rd().fromHistory}>
                      <div class={O.row}>
                        <span class={O.badge.quiet}>from what you did before</span>
                        <span class={O.spacer} />
                        <button
                          class={O.linkish}
                          onClick={() => {
                            setIgnoreHistory(true);
                            const { pocket_name: _drop, ...rest } = known();
                            void run(rest);
                          }}
                        >
                          change
                        </button>
                      </div>
                    </Show>

                    <button
                      class={`${O.action.primary} ${O.actionWide}`}
                      onClick={() => void confirm(rd())}
                      disabled={busy()}
                    >
                      <Show when={busy()} fallback="record it">
                        <span class={O.working} /> asking the gate…
                      </Show>
                    </button>
                    <button class={`${O.action.quiet} ${O.actionWide}`} onClick={reset}>
                      not now
                    </button>
                  </div>
                );
              })()}
            </Show>

            <Show when={s().status === "cannotInfer"}>
              <p class={O.body}>
                {(s() as Extract<InferenceDto, { status: "cannotInfer" }>).why}
              </p>
              <button class={`${O.action.quiet} ${O.actionWide}`} onClick={reset}>
                try again
              </button>
            </Show>
          </div>
        )}
      </Show>

      {/* ═══ the gate's answer ══════════════════════════════════════════ */}
      <Show when={verdict()}>
        {(v) => (
          <div class={O.stack}>
            <div class={O.verdict[v().verdict === "admitted" ? "admitted" : "refused"]}>
              <div class={O.verdictWord[v().verdict === "admitted" ? "admitted" : "refused"]}>
                {v().verdict === "admitted" ? "RECORDED" : v().verdict.toUpperCase()}
              </div>
              <Show when={v().reason}>
                {(r) => <p class={O.caption}>{r()}</p>}
              </Show>
              <Show when={v().verdict !== "admitted"}>
                <p class={O.caption}>nothing moved · nothing logged</p>
              </Show>
            </div>

            {/* ★★★ "remember this format" — only after a real success, and only
                for a captured message. A learned SPEND still asks next time. */}
            <Show when={v().verdict === "admitted" && props.messageId}>
              <Show
                when={!learned()}
                fallback={<p class={O.caption}>learned · this shape is recognised from now on</p>}
              >
                <Show when={confirmed()}>
                  {(p) => (
                    <button
                      class={`${O.action.secondary} ${O.actionWide}`}
                      onClick={() => void remember(v().operator, p())}
                    >
                      remember this format
                    </button>
                  )}
                </Show>
              </Show>
            </Show>

            <button
              class={`${O.action.quiet} ${O.actionWide}`}
              onClick={() => {
                setVerdict(null);
                setLearned(null);
                setConfirmed(null);
              }}
            >
              done
            </button>
          </div>
        )}
      </Show>

      <Show when={failure()}>
        {(f) => <div class={O.errorBox}>{f()}</div>}
      </Show>
    </div>
  );
}
