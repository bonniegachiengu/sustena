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
  Field,
  Label,
  Meta,
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
              ★ The source is the <strong>sender</strong>, and it decides which rules run. A
              message tagged <code>kcb</code> is never parsed by M-Pesa's rules — several real KCB
              messages say <em>M-PESA</em> in their own wording, and the body must never promote a
              message out of the set its sender established.
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
                        nothing stored · no row, no payload, not even a hash
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
              A source is only ever reported stale if it <strong>declared a cadence</strong> and
              missed it. One that never declared a rhythm is never flagged — guessing one would be
              exactly the fabricated state this queue exists to prevent.
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
            ★★★ <strong>A refused message is a count and nothing more.</strong> It was never
            written — no row, no raw text, not even a dedup hash, because a hash of an OTP is still
            a record that the OTP existed.
          </Caption>

          <Note gap="lg">
            <Label>messages</Label>
          </Note>
          <Show
            when={(data()?.messages.length ?? 0) > 0}
            fallback={<Empty>nothing captured yet · paste a message on the left</Empty>}
          >
            <For each={[...(data()?.messages ?? [])].reverse()}>
              {(m) => (
                <div class={S.pocketBlock}>
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
                    <Cluster>
                      <Chip onClick={() => void resolve(m)}>mark handled</Chip>
                      <Caption>
                        which pocket this belongs to is yours to decide — Orchie is where that
                        happens
                      </Caption>
                    </Cluster>
                  </Show>
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
              ★★ Every rule is <strong>declared data</strong>, not code — which is what makes a
              correction something a person can author instead of a release. ★★★ Only an{" "}
              <em>income</em> rule may auto-apply: a learned spend rule carries no operator at all,
              so a correction can never teach the system to spend on your behalf.
            </Caption>
          </Note>
        </Card>
      </Column>
    </Split>
  );
}
