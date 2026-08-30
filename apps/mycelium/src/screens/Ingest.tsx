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
import { Classify } from "./Orchie";

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
                  <Show when={m.needsAttention}>
                    {/* (*) The chips sit inside their own click boundary. The
                        block around them is a door to the classifier, and
                        "mark handled" is a different decision -- letting it
                        bubble would open the flow the person just declined. */}
                    <div onClick={(e) => e.stopPropagation()}>
                    <Cluster>
                      {/* (*) The dead end this replaced read "you choose the
                          pocket in Orchie" -- a surface telling a person that
                          the thing they came to do happens somewhere else. It
                          happens here now, in the same classifier Orchie and the
                          notification tap open. */}
                      <Chip onClick={() => setClassifying(m.id)}>classify</Chip>
                      <Chip onClick={() => void resolve(m)}>mark handled</Chip>
                      <Caption>filed here · nothing moves until you confirm</Caption>
                    </Cluster>
                    </div>
                  </Show>
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
            }}
          />
        </Modal>
      )}
    </Show>
    </>
  );
}
