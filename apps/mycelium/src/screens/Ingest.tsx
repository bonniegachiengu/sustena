/**
 * INGEST — the capture queue, real.
 *
 * ★★★ This screen used to be an honest absence: *there is no transducer in this
 * engine*. There is now. `τ` is in `sustena-core`, the 22 shipped rules are
 * declared data, and the queue below is what arrived.
 *
 * ★★★ **A message carrying a secret never appears here.** It is refused before
 * anything is written — no row, no raw payload, not even a dedup hash. The only
 * thing this screen can show about one is a **count**, and it says so, because
 * a queue that displayed the OTP it refused would have defeated the refusal.
 *
 * ★★ The tiers are shown as they are, never collapsed. *Understood* and
 * *actionable* are different: an outbound payment is `parsed_unmapped` on
 * purpose, because which pocket it belongs to is a person's decision and a
 * heuristic for it would be inventing a spending decision on their behalf.
 */
import { createMemo, createResource, createSignal, For, Show } from "solid-js";

import { IntakeStart } from "../components/IntakeStart";

import { onPulse } from "../lib/pulse";
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
  Field,
  Label,
  Meta,
  Modal,
  Note,
  NoteRow,
  Row,
  Spacer,
  Split,
  TelemetryCell,
  TelemetryStrip,
  Value,
  sx as S,
} from "../ui";
import { engine, fmt, type CaptureResult, type MessageDto } from "../lib/engine";
import { world } from "../lib/live";
import { Classify, TrainFlow } from "./Orchie";

/** Tier → how it reads. ★ One place, so a tier cannot mean two things. */
const TIER: Record<string, { tone: "ok" | "warn" | "danger" | "quiet"; says: string }> = {
  mapped: { tone: "ok", says: "understood and routed" },
  parsed_unmapped: { tone: "warn", says: "understood · needs a pocket decision" },
  informational: { tone: "quiet", says: "recognised · nothing moved" },
  unparsed: { tone: "warn", says: "no rule recognised this shape" },
};

export default function Ingest() {
  const sustain = () => world.selected ?? "";
  const [data, { refetch }] = createResource(sustain, (id) => engine.ingest(id));

  // ★★★ §III: relayed change, refreshed surface. A capture classified anywhere -- here, from a card, from a notification -- leaves this list.
  onPulse(refetch);

  /** How many rows paint at once. See the note beside "show more". */
  const PAGE = 40;
  const [shown, setShown] = createSignal(PAGE);
  /** Which message the classifier is open on, if any. */
  const [classifying, setClassifying] = createSignal<string | null>(null);
  /**
   * The message he chose to teach Orchie the shape of.
   *
   * (*) The whole message, not its id: teaching needs the raw text, and the
   * row already holds it. Looking it up again to get back what was in hand
   * would be a round trip for nothing.
   */
  /**
   * The message he chose to teach Orchie the shape of.
   *
   * (*) Just the id and the text, which is all teaching needs. It used to hold
   * the whole `MessageDto`, and that made the review card below unable to open
   * the same flow -- it has a row, not a message. Narrowing the signal to what
   * is actually used let both doors lead to the same place.
   */
  const [training, setTraining] = createSignal<{ id: string; raw: string } | null>(null);

  /**
   * What was filed without anyone being asked.
   *
   * (*) Its own resource rather than part of the main load: it answers a
   * different question ("what did Orchie do on its own?") and a person opens
   * this screen far more often than they need to audit it.
   */
  const [review, { refetch: refetchReview }] = createResource(
    () => sustain(),
    (id: string) => (id ? engine.autoFiled(id, 40) : Promise.resolve([])),
  );
  const doubted = () => (review() ?? []).filter((r) => r.doubts.length > 0);

  /**
   * (*) **Newest first.** The message a person still remembers is the cheapest
   * to answer, and an oldest-first backlog asks the hardest question first.
   * `current()` is ordered by seq ascending, so reversing it is the recency
   * order rather than a re-sort.
   */
  const ordered = createMemo(() => [...(data()?.messages ?? [])].reverse());
  const page = createMemo(() => ordered().slice(0, shown()));
  const waiting = createMemo(() => ordered().filter((m) => m.needsAttention).length);

  const [source, setSource] = createSignal("mpesa");
  const [raw, setRaw] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [last, setLast] = createSignal<CaptureResult | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);

  const capture = async () => {
    if (!sustain() || raw().trim() === "") return;
    setFailure(null);
    setBusy(true);
    try {
      const r = await engine.capture(sustain(), source(), raw());
      setLast(r);
      // ★ Cleared either way. The text does not linger in a field after a
      //   rejection any more than it lingers on disk.
      setRaw("");
      await refetch();
    } catch (e) {
      setFailure(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  const resolve = async (m: MessageDto) => {
    await engine.resolveMessage(m.id);
    await refetch();
  };

  return (
    <>
    <Split>
      <Column>
        <Card
          title="capture"
          right={<Meta>{world.sustains[sustain()]?.summary.label ?? "—"}</Meta>}
        >
          <Field label="source">
            <select
              class={S.select}
              value={source()}
              onChange={(e) => setSource(e.currentTarget.value)}
            >
              <For each={["mpesa", "kcb"]}>{(s) => <option value={s}>{s}</option>}</For>
            </select>
          </Field>
          <Note>
            <Field label="the message, exactly as it arrived">
              <textarea
                class={S.input}
                rows="4"
                value={raw()}
                onInput={(e) => setRaw(e.currentTarget.value)}
              />
            </Field>
          </Note>
          <Note gap="lg">
            <Cluster>
              <Button onClick={capture} disabled={busy() || raw().trim() === ""}>
                {busy() ? "reading…" : "capture"}
              </Button>
            </Cluster>
          </Note>

          <Note>
            <Caption>
              Pick the source by who sent the message, not by what it says. Several real KCB
              messages mention M-PESA in their wording, and they are still KCB.
            </Caption>
          </Note>

          <Show when={failure()}>
            {(f) => (
              <Note>
                <ErrorState>{f()}</ErrorState>
              </Note>
            )}
          </Show>

          <Show when={last()}>
            {(r) => (
              <Note gap="lg">
                <Show
                  when={r().kind !== "rejected"}
                  fallback={
                    <div class={S.verdictBox.refused}>
                      <Cluster>
                        <span class={S.verdictWord.refused}>REJECTED</span>
                        <Meta>the secret gate</Meta>
                      </Cluster>
                      <p class={S.reason}>{(r() as { reason: string }).reason}</p>
                      {/* ★★★ The line that matters. */}
                      <div class={S.reasonCode}>
                        nothing was stored
                      </div>
                    </div>
                  }
                >
                  <>
                    {(() => {
                      const m = () => (r() as { message: MessageDto }).message;
                      return (
                        <div
                        class={
                          m().status === "mapped" ? S.verdictBox.admitted : S.verdictBox.deferred
                        }
                      >
                        <Cluster>
                          <span class={S.label}>
                            {r().kind === "duplicate" ? "ALREADY CAPTURED" : m().status}
                          </span>
                          <Meta>{m().parserName || "no rule matched"}</Meta>
                        </Cluster>
                        <p class={S.reason}>{m().reason}</p>
                        <Show when={m().applied}>
                          <div class={S.reasonCode}>
                            applied · {m().operator} ran through the gate
                          </div>
                        </Show>
                        <Show when={m().gateReason}>
                          {(g) => <div class={S.reasonCode}>the gate refused it · {g()}</div>}
                        </Show>
                        </div>
                      );
                    })()}
                  </>
                </Show>
              </Note>
            )}
          </Show>
        </Card>

        {/* ★★★ Where the record begins. Placed beside the sources, because
            "what do we listen to" and "from when" are the same question asked
            twice, and splitting them across screens is how a person ends up
            with a boundary they cannot find. */}
        <Card title="where the record begins">
          <IntakeStart face="cockpit" onChanged={() => void refetch()} />
        </Card>

        <Card title="sources">
          <Show
            when={(data()?.sources.length ?? 0) > 0}
            fallback={<Empty>nothing has captured yet</Empty>}
          >
            <For each={data()!.sources}>
              {(s) => (
                <Row>
                  <Value>{s.label}</Value>
                  <Meta>{s.captures} captured</Meta>
                  <Spacer />
                  {/* ★ Never a guessed cadence, so never a guessed staleness. */}
                  <Meta>
                    {s.expectedIntervalMinutes
                      ? `every ${s.expectedIntervalMinutes} min`
                      : "no cadence declared"}
                  </Meta>
                </Row>
              )}
            </For>
          </Show>
          <Note>
            <Caption>
              A source is only shown as stale if you told it how often to expect a message
              and it missed. Sources with no expected rhythm are never flagged.
            </Caption>
          </Note>
        </Card>
      </Column>

      <Column>
        <Card
          title="the queue"
          right={
            <>
              <Show when={(data()?.needsAttention ?? 0) > 0}>
                <Badge tone="warn">{data()!.needsAttention} need you</Badge>
              </Show>
              {/* ★★★ A count, and nothing else. */}
              <Show when={(data()?.rejected ?? 0) > 0}>
                <Badge tone="danger">{data()!.rejected} refused</Badge>
              </Show>
            </>
          }
          scroll
        >
          <TelemetryStrip>
            <TelemetryCell label="captured">{data()?.messages.length ?? 0}</TelemetryCell>
            <TelemetryCell label="need a decision" tone={(data()?.needsAttention ?? 0) > 0 ? "warn" : "ok"}>
              {data()?.needsAttention ?? 0}
            </TelemetryCell>
            <TelemetryCell label="rules in force">{data()?.rules.length ?? 0}</TelemetryCell>
            <TelemetryCell label="refused for a secret" tone="danger">
              {data()?.rejected ?? 0}
            </TelemetryCell>
          </TelemetryStrip>
          <Caption>
            <strong>A refused message is only ever a count.</strong> Nothing about it is
            stored, because it may carry a one-time code.
          </Caption>

          <Note gap="lg">
            <Label>messages</Label>
            {/* (*) The counts are of EVERYTHING, not of what is rendered. A
                backlog of two and a half thousand is a fact about the household
                and it must not shrink because a list was capped. */}
            <Caption>
              {waiting()} waiting on a decision · {(data()?.messages.length ?? 0)} captured
              {shown() < ordered().length ? ` · showing the newest ${shown()}` : ""}
            </Caption>
          </Note>
          <Show
            when={ordered().length > 0}
            fallback={<Empty>nothing captured yet · paste a message on the left</Empty>}
          >
            <For each={page()}>
              {(m) => (
                /* (*) Rule 2. The whole block is the door, not a chip inside it:
                   a captured message IS a decision waiting to be made, and the
                   thing a person wants when they look at one is to make it. */
                <div
                  class={S.pocketBlock}
                  role={m.needsAttention ? "button" : undefined}
                  tabindex={m.needsAttention ? 0 : undefined}
                  style={m.needsAttention ? { cursor: "pointer" } : undefined}
                  onClick={m.needsAttention ? () => setClassifying(m.id) : undefined}
                >
                  <Cluster>
                    <Badge tone={TIER[m.status]?.tone ?? "quiet"}>{m.status}</Badge>
                    <Meta>{m.parserName || "—"}</Meta>
                    <Spacer />
                    <Show when={m.amount !== null}>
                      <Value>{fmt(m.amount)}</Value>
                    </Show>
                  </Cluster>
                  <Caption>{TIER[m.status]?.says ?? m.status}</Caption>
                  <Show when={m.counterparty}>
                    {(c) => (
                      <Caption>
                        {c()}
                        {m.direction ? ` · ${m.direction}` : ""}
                      </Caption>
                    )}
                  </Show>
                  {/* ★ The raw text, kept deliberately: correcting a
                      classification needs to see what actually arrived. */}
                  <pre class={S.codeBlock}>{m.rawPayload}</pre>
                  <Show when={m.applied}>
                    <NoteRow tone="ok">
                      <Caption>
                        applied · <code>{m.operator}</code> ran through the gate
                      </Caption>
                    </NoteRow>
                  </Show>
                  <Show when={m.gateReason}>
                    {(g) => (
                      <NoteRow tone="danger">
                        <Caption>the gate refused it · {g()}</Caption>
                      </NoteRow>
                    )}
                  </Show>
                  {/* (*) The chips sit inside their own click boundary. The
                      block around them is a door to the classifier, and
                      "mark handled" is a different decision -- letting it
                      bubble would open the flow the person just declined. */}
                  <div onClick={(e) => e.stopPropagation()}>
                    <Cluster>
                      <Show when={m.needsAttention}>
                        {/* (*) The dead end this replaced read "you choose the
                            pocket in Orchie" -- a surface telling a person that
                            the thing they came to do happens somewhere else. It
                            happens here now, in the same classifier Orchie and the
                            notification tap open. */}
                        <Chip onClick={() => setClassifying(m.id)}>classify</Chip>
                        <Chip onClick={() => void resolve(m)}>mark handled</Chip>
                      </Show>
                      {/* (*) Offered on EVERY row, not only the ones nothing
                          read. A message the parser read WRONGLY is worth
                          teaching too, and it is the row he is looking at when
                          he notices -- so the way to fix it belongs here. */}
                      <Chip onClick={() => setTraining({ id: m.id, raw: m.rawPayload })}>train from this</Chip>
                      <Show when={m.needsAttention}>
                        <Caption>filed here · nothing moves until you confirm</Caption>
                      </Show>
                    </Cluster>
                  </div>
                </div>
              )}
            </For>
            {/* (*) Progressive rather than paged. Two and a half thousand
                messages rendered at once is the freeze this codebase already
                met with a large backfill, and the answer there was the same:
                bound what paints, keep the count honest. */}
            <Show when={shown() < ordered().length}>
              <Cluster>
                <Chip onClick={() => setShown(shown() + PAGE)}>
                  show {Math.min(PAGE, ordered().length - shown())} more
                </Chip>
                <Caption>{ordered().length - shown()} older still below</Caption>
              </Cluster>
            </Show>
          </Show>
        </Card>

        {/* (*) The safety guard. As Orchie files more on its own -- income
            that maps itself, a taught shape across a whole cluster, a pocket
            pre-filled from a route -- more lands in his books that nobody
            looked at. Most of it is right, and that is exactly why a wrong
            one is invisible: it never needed him, so it never reached him. */}
        <Card
          title="filed without asking"
          right={
            <Meta>
              {doubted().length > 0
                ? `${doubted().length} to check`
                : `${(review() ?? []).length}`}
            </Meta>
          }
        >
          <Show
            when={(review() ?? []).length > 0}
            fallback={<Empty>nothing has filed itself yet</Empty>}
          >
            <Show when={doubted().length === 0}>
              <Caption>nothing here disagrees with itself</Caption>
            </Show>
            <For each={review()}>
              {(r) => (
                <div class={S.pocketBlock}>
                  <Cluster>
                    <Badge tone={r.doubts.length > 0 ? "danger" : "quiet"}>
                      {r.doubts.length > 0 ? "check this" : "filed"}
                    </Badge>
                    <Meta>{r.when}</Meta>
                    <Spacer />
                    <Value>{fmt(r.amount)}</Value>
                  </Cluster>
                  <Caption>{r.what}</Caption>
                  {/* (*) The doubt's own words, not a re-phrasing. Each names
                      BOTH sides of its disagreement, so he can settle it by
                      reading rather than by trusting the guard. */}
                  <For each={r.doubts}>
                    {(d) => (
                      <NoteRow tone="danger">
                        <Caption>{d}</Caption>
                      </NoteRow>
                    )}
                  </For>
                  <pre class={S.codeBlock}>{r.raw}</pre>
                  <div onClick={(e) => e.stopPropagation()}>
                    <Cluster>
                      {/* (*) One tap to put it right, through the ordinary
                          append-only correction. The first filing stays on the
                          record and the change is added after it. */}
                      <Chip onClick={() => setClassifying(r.messageId)}>fix this</Chip>
                      <Chip onClick={() => setTraining({ id: r.messageId, raw: r.raw })}>train from this</Chip>
                    </Cluster>
                  </div>
                </div>
              )}
            </For>
          </Show>
        </Card>

        <Card title="the rule library" right={<Meta>{data()?.rules.length ?? 0}</Meta>}>
          <Show when={(data()?.rules.length ?? 0) > 0} fallback={<Empty>no rules loaded</Empty>}>
            <For each={data()!.rules}>
              {(r) => (
                <Row>
                  <Value>{r.id}</Value>
                  <Meta>{r.source}</Meta>
                  <Spacer />
                  <Meta>{r.status}</Meta>
                  <Badge tone={r.trust === "shipped" ? "quiet" : "ok"}>
                    {r.trust === "shipped" ? "shipped" : "yours"}
                  </Badge>
                </Row>
              )}
            </For>
          </Show>
          <Note>
            <Caption>
              Rules are data, so you can correct one yourself. Only income rules apply on
              their own. A learned spend rule always asks you first.
            </Caption>
          </Note>
        </Card>
      </Column>
    </Split>

    {/* (*) ONE classifier, another door. This is the same `Classify` the
        Orchie feed renders and the notification tap opens -- imported, not
        reimplemented, so a change to how classifying works reaches every
        entry point at once. */}
    {/* (*) The same teaching the Orchie feed and its classify card open --
        imported, not reimplemented, so what a learned rule means is one
        answer given in one place. */}
    <Show when={training()}>
      {(m) => (
        <Modal title="teach this shape" onClose={() => setTraining(null)}>
          <TrainFlow
            sustain={sustain()}
            messageId={m().id}
            raw={m().raw}
            onDone={() => {
              void refetch();
              void refetchReview();
            }}
            onClose={() => setTraining(null)}
          />
        </Modal>
      )}
    </Show>

    <Show when={classifying()}>
      {(id) => (
        <Modal title="classify" onClose={() => setClassifying(null)}>
          <Classify
            sustain={sustain()}
            messageId={id()}
            autoStart
            backfill
            onDone={() => {
              setClassifying(null);
              void refetch();
              // (*) The guard has to forget what he just settled, or it goes
              //     on asking about a filing he has already put right.
              void refetchReview();
            }}
          />
        </Modal>
      )}
    </Show>
    </>
  );
}
