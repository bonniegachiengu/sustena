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
 * ★★★ **Every card can say why it is here** — its real urgency, whether that
 * urgency was *measured*, its relevance, its score, and what made it eligible.
 * And what stayed quiet says which kind of quiet it was: **withdrawn** (nothing
 * to say, carries a reason) or **outranked** (considered, carries a score).
 * Those are different facts and the line tells them apart.
 *
 * ★★ The capture flow asks **one question at a time**, with options that are
 * the household's own pocket names, and stops at a confirmation every time. A
 * history pre-fill saves the tap, never the confirm.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import {
  Badge,
  Button,
  Caption,
  Card,
  Chip,
  Cluster,
  Column,
  Empty,
  ErrorState,
  Label,
  Meta,
  Meter,
  Note,
  NoteRow,
  Readout,
  Row,
  Spacer,
  Value,
  sx as S,
  layout as L,
} from "../ui";
import {
  engine,
  fmt,
  type FeedDto,
  type GateResult,
  type InferenceDto,
  type JsonValue,
} from "../lib/engine";
import { world } from "../lib/live";

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

export default function Orchie() {
  const sustain = () => world.selected ?? "";
  const [feed, { refetch }] = createResource(sustain, (id) => engine.feed(id, null));
  const [showQuiet, setShowQuiet] = createSignal(false);

  return (
    <div class={L.orchieFrame}>
      <div class={L.orchieColumn}>
        <Cluster>
          <span class={S.brand}>ORCHIE</span>
          <Spacer />
          <Meta>{feed()?.label ?? "—"}</Meta>
        </Cluster>

        <Show
          when={feed()}
          fallback={
            <Show when={feed.error} fallback={<Empty>reading the household…</Empty>}>
              {(e) => (
                <ErrorState>
                  the engine is unreachable — {String(e())}
                  <br />
                  nothing is shown rather than something stale.
                </ErrorState>
              )}
            </Show>
          }
        >
          {(f) => (
            <>
              {/* ── the calm read ──────────────────────────────────────── */}
              <Summary feed={f()} />

              {/* ── the things that need you ───────────────────────────── */}
              <Show when={f().attention.length > 0}>
                <Card title="needs you" right={<Meta>{f().attention.length}</Meta>}>
                  <For each={f().attention}>
                    {(a) => (
                      <NoteRow tone={a.severity === "danger" ? "danger" : "warn"}>
                        <Value>{a.what}</Value>
                        <Caption>{a.why}</Caption>
                        <Show when={a.messageId}>
                          {(id) => (
                            <Classify
                              sustain={f().sustainId}
                              messageId={id()}
                              onDone={() => void refetch()}
                            />
                          )}
                        </Show>
                      </NoteRow>
                    )}
                  </For>
                </Card>
              </Show>

              {/* ── the ranked feed ────────────────────────────────────── */}
              <Show
                when={f().cards.length > 0}
                fallback={<Empty>nothing needs you right now</Empty>}
              >
                <For each={f().cards}>
                  {(c) => (
                    <Card
                      title={TITLE[c.id] ?? c.id}
                      right={
                        <Show when={c.urgency > 0}>
                          <Badge tone={c.urgency > 0.5 ? "danger" : "warn"}>
                            {Math.round(c.urgency * 100)}%
                          </Badge>
                        </Show>
                      }
                    >
                      <CardBody card={c} feed={f()} onDone={() => void refetch()} />
                      <Why card={c} />
                    </Card>
                  )}
                </For>
              </Show>

              {/* ── what stayed quiet ──────────────────────────────────── */}
              <Show when={f().quiet.length > 0}>
                <Note>
                  <button class={S.chip} onClick={() => setShowQuiet((q) => !q)}>
                    {f().quiet.length} other {f().quiet.length === 1 ? "thing" : "things"} stayed
                    quiet
                  </button>
                </Note>
                <Show when={showQuiet()}>
                  <Card title="what stayed quiet">
                    <For each={f().quiet}>
                      {(q) => (
                        <>
                          <Row>
                            <Value>{TITLE[q.id] ?? q.id}</Value>
                            <Spacer />
                            {/* ★★ Withdrawn and outranked are different facts. */}
                            <Show
                              when={q.withdrew}
                              fallback={<Meta>outranked · score {q.score?.toFixed(2)}</Meta>}
                            >
                              <Meta>nothing to say</Meta>
                            </Show>
                          </Row>
                          {/* ★ The engine's own reason, kept where it belongs:
                              available, but under the plain line rather than
                              instead of it. */}
                          <Show when={q.reason}>
                            <Caption>{q.reason}</Caption>
                          </Show>
                        </>
                      )}
                    </For>
                    <Note>
                      <Caption>
                        A card that <strong>withdrew</strong> had nothing to say and carries a
                        reason; one that was <strong>outranked</strong> was considered and carries
                        a score. {f().spent} of {f().budget} attention spent across{" "}
                        {f().candidatesConsidered} candidates.
                      </Caption>
                    </Note>
                  </Card>
                </Show>
              </Show>

              {/* ── narrate anything ───────────────────────────────────── */}
              <Card title="what happened?">
                <Classify sustain={f().sustainId} messageId={null} onDone={() => void refetch()} />
              </Card>
            </>
          )}
        </Show>
      </div>
    </div>
  );
}

/** The calm read: one plain figure, never the machinery. */
function Summary(props: { feed: FeedDto }) {
  const total = () =>
    props.feed.rollup?.aggregates.find((a) => a.childPath === "finances.liquid.balance");
  return (
    <Card title="the household">
      <Show
        when={total()?.grounded}
        fallback={<Readout label="liquid" big>{fmt(props.feed.liquid)}</Readout>}
      >
        <Readout label="everything, together" big>
          {fmt(total()?.value ?? null)}
        </Readout>
        <Caption>
          folded across {total()?.included.filter((c) => !c.isHousehold).length ?? 0} member
          {(total()?.included.filter((c) => !c.isHousehold).length ?? 0) === 1 ? "" : "s"} and this
          household's own, fresh — nothing stored
        </Caption>
        {/* ★★★ An exclusion is never silent. A figure that quietly counted an
            unreadable member as zero would read as calm and be wrong; this
            says who was left out, so the total is read for what it is. */}
        <Show when={(total()?.excluded.length ?? 0) > 0}>
          <Caption>
            {total()?.excluded.length} left out, not counted as zero:{" "}
            {total()
              ?.excluded.map((e) => e.label)
              .join(", ")}
          </Caption>
        </Show>
      </Show>
    </Card>
  );
}

/** ★★★ "Why am I seeing this?" — real numbers, never a rationalisation. */
function Why(props: { card: { urgency: number; measured: boolean; basis: string; relevance: number; score: number; cost: number; eligibility: string } }) {
  const [open, setOpen] = createSignal(false);
  return (
    <Note>
      <button class={S.chip} onClick={() => setOpen((o) => !o)}>
        why am I seeing this?
      </button>
      <Show when={open()}>
        <Note gap="sm">
          <Caption>{props.card.eligibility}</Caption>
          <Show
            when={props.card.measured}
            fallback={
              /* ★★ A 0 on an undeclared basis is silence, not safety. */
              <Caption>
                urgency <strong>not measured</strong> — {props.card.basis}
              </Caption>
            }
          >
            <Caption>urgency {props.card.urgency.toFixed(2)} · {props.card.basis}</Caption>
          </Show>
          <Caption>
            relevance {props.card.relevance.toFixed(2)} · score {props.card.score.toFixed(2)} ·
            costs {props.card.cost} of the attention budget
          </Caption>
        </Note>
      </Show>
    </Note>
  );
}

/** What a card draws. ★ Keyed off its declared `render` tag. */
function CardBody(props: { card: { id: string; render: string }; feed: FeedDto; onDone: () => void }) {
  return (
    <Show
      when={props.card.render === "pocket_strain"}
      fallback={<PlainCard feed={props.feed} card={props.card} />}
    >
      <Readout label={str(props.feed, "worst_pocket")}>
        {fmt(num(props.feed, "worst_spent"))} of {fmt(num(props.feed, "worst_allocated"))}
      </Readout>
      <Meter
        fraction={
          num(props.feed, "worst_allocated") > 0
            ? num(props.feed, "worst_spent") / num(props.feed, "worst_allocated")
            : null
        }
      />
      <Caption>the limit is the one you set for it</Caption>
    </Show>
  );
}

function PlainCard(props: { feed: FeedDto; card: { id: string } }) {
  return (
    <Show
      when={props.card.id === "classify_capture"}
      fallback={<Readout label="liquid">{fmt(num(props.feed, "liquid"))}</Readout>}
    >
      <Readout label="waiting on you">{str(props.feed, "unclassified")}</Readout>
      <Caption>each one needs a pocket before it can be recorded</Caption>
    </Show>
  );
}

/**
 * ★★★ The signature flow: an effect → `infer` → **one question at a time** →
 * confirm → the operator through the real gate.
 */
function Classify(props: { sustain: string; messageId: string | null; onDone: () => void }) {
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
    }
  };

  const answer = (field: string, value: string) => void run({ ...known(), [field]: value });

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
        setStep(null);
        setKnown({});
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

  return (
    <div class={S.note.sm}>
      <Show when={!props.messageId && !step()}>
        <Cluster>
          <input
            class={S.input}
            placeholder="spent 500 on food"
            value={text()}
            onInput={(e) => setText(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void run({});
            }}
          />
        </Cluster>
        <Note gap="sm">
          <Button onClick={() => void run({})} disabled={busy() || text().trim() === ""}>
            {busy() ? "working it out…" : "go"}
          </Button>
        </Note>
      </Show>

      <Show when={props.messageId && !step()}>
        <Cluster>
          <Chip onClick={() => void run({})} disabled={busy()}>
            {busy() ? "working it out…" : "classify this"}
          </Chip>
        </Cluster>
      </Show>

      <Show when={step()}>
        {(s) => (
          <Note gap="sm">
            {/* one question at a time */}
            <Show when={s().status === "needsDisambiguation"}>
              <>
                {(() => {
                  const q = () => s() as Extract<InferenceDto, { status: "needsDisambiguation" }>;
                  return (
                    <>
                    <Label>{q().question}</Label>
                    <Caption>{q().why}</Caption>
                    {/* ★ options: null means the answer is not a tap. */}
                    <Show
                      when={q().options}
                      fallback={
                        <Cluster>
                          <input
                            class={S.input}
                            inputmode="decimal"
                            placeholder="0"
                            onKeyDown={(e) => {
                              if (e.key === "Enter")
                                answer(q().field, e.currentTarget.value);
                            }}
                          />
                        </Cluster>
                      }
                    >
                      {(opts) => (
                        <Cluster>
                          <For each={opts()}>
                            {(o) => (
                              <Chip disabled={busy()} onClick={() => answer(q().field, o.value)}>
                                {o.label}
                              </Chip>
                            )}
                          </For>
                        </Cluster>
                      )}
                    </Show>
                    </>
                  );
                })()}
              </>
            </Show>

            <Show when={s().status === "ready"}>
              <>
                {(() => {
                  const rd = () => s() as Extract<InferenceDto, { status: "ready" }>;
                  return (
                    <>
                    <Caption>{rd().why}</Caption>
                    {/* ★★ A history pre-fill is never silent. */}
                    <Show when={rd().fromHistory}>
                      <Cluster>
                        <Badge tone="quiet">from what you did before</Badge>
                        <Chip
                          onClick={() => {
                            setIgnoreHistory(true);
                            const { pocket_name: _drop, ...rest } = known();
                            void run(rest);
                          }}
                        >
                          change
                        </Chip>
                      </Cluster>
                    </Show>
                    <Note gap="sm">
                      <Cluster>
                        <Button onClick={() => void confirm(rd())} disabled={busy()}>
                          {busy() ? "asking the gate…" : "confirm"}
                        </Button>
                        <Chip onClick={() => { setStep(null); setKnown({}); }}>cancel</Chip>
                      </Cluster>
                    </Note>
                    </>
                  );
                })()}
              </>
            </Show>

            <Show when={s().status === "cannotInfer"}>
              <Caption>{(s() as Extract<InferenceDto, { status: "cannotInfer" }>).why}</Caption>
            </Show>
          </Note>
        )}
      </Show>

      <Show when={verdict()}>
        {(v) => (
          <Note gap="sm">
            <div class={S.verdictBox[v().verdict]}>
              <span class={S.verdictWord[v().verdict]}>
                {v().verdict === "admitted" ? "RECORDED" : v().verdict.toUpperCase()}
              </span>
              <Show when={v().reason}>{(r) => <p class={S.reason}>{r()}</p>}</Show>
              <Show when={v().verdict !== "admitted"}>
                <div class={S.reasonCode}>nothing moved · nothing logged</div>
              </Show>
            </div>
            {/* ★★★ "remember this format" — only after a real success, and only
                for a captured message. A learned SPEND still asks next time. */}
            <Show when={v().verdict === "admitted" && props.messageId}>
              <Cluster>
                <Show
                  when={!learned()}
                  fallback={<Caption>learned · this shape is recognised from now on</Caption>}
                >
                  <Show when={confirmed()}>
                    {(p) => (
                      <Chip onClick={() => void remember(v().operator, p())}>
                        remember this format
                      </Chip>
                    )}
                  </Show>
                </Show>
              </Cluster>
            </Show>
          </Note>
        )}
      </Show>

      <Show when={failure()}>
        {(f) => (
          <Note gap="sm">
            <ErrorState>{f()}</ErrorState>
          </Note>
        )}
      </Show>
    </div>
  );
}
