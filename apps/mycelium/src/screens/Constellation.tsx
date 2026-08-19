/**
 * CONSTELLATION — the live network of Sustains.
 *
 * ★★★ Every node is a **real persisted Sustain** read from the world summaries;
 * nothing here is hardcoded, including how many there are. Vitals come from the
 * summary, which the push channel keeps current — so the graph is live without
 * hydrating a single state document.
 *
 * ★★ The gate stream is the real-time showcase: it is fed **only** by the
 * channel. An entry appears because the engine decided something, never because
 * this screen asked.
 */
import { createMemo, For, Show } from "solid-js";
import {
  Badge,
  Caption,
  Card,
  Column,
  Empty,
  Fill,
  Meta,
  Note,
  NoteRow,
  Row,
  Split,
  TelemetryCell,
  TelemetryStrip,
  Value,
  vars,
  sx as S,
} from "../ui";
import { fmt } from "../lib/engine";
import { ATTENTION_AT, attentionAcross, attentionFor, world } from "../lib/live";

const RADIUS = 132;
const CENTER = { x: 190, y: 168 };

const toneOf = (t: "ok" | "warn" | "danger") =>
  t === "danger" ? vars.color.danger : t === "warn" ? vars.color.warn : vars.color.teal;

export default function Constellation(props: { onOpen: (id: string) => void }) {
  const roots = createMemo(() =>
    world.order.map((k) => world.sustains[k]!).filter((x) => x && x.summary.parent === null),
  );

  /** The graph: one root at the centre, its children on a ring. */
  const layout = createMemo(() => {
    const root = roots()[0];
    if (!root) return { root: null, kids: [] as { id: string; x: number; y: number }[] };
    const kids = world.order
      .map((k) => world.sustains[k]!)
      .filter((x) => x && x.summary.parent === root.summary.id);
    const step = (Math.PI * 2) / Math.max(kids.length, 1);
    return {
      root,
      kids: kids.map((k, i) => ({
        id: k.summary.id,
        x: CENTER.x + RADIUS * Math.cos(i * step - Math.PI / 2),
        y: CENTER.y + RADIUS * Math.sin(i * step - Math.PI / 2),
      })),
    };
  });

  /** ★ Only what can be honestly computed from summaries alone. */
  const telemetry = createMemo(() => {
    const all = world.order.map((k) => world.sustains[k]!).filter(Boolean);
    const events = all.reduce((n, x) => n + x.summary.events, 0);
    const rules = all.reduce((n, x) => n + x.summary.constraints.length, 0);
    const broken = all.reduce((n, x) => n + x.summary.constraints.filter((c) => !c.holds).length, 0);
    return { sustains: all.length, events, rules, broken };
  });

  const attention = createMemo(attentionAcross);
  const nodeAt = (id: string) => world.sustains[id];

  /**
   * ★★ The household total, taken from whichever root Sustain actually
   * declares one — never assembled here. If no Sustain declares a total there
   * is none to show, and the strip says so rather than inventing a sum.
   */
  const householdTotal = createMemo(() => {
    for (const id of world.order) {
      const agg = world.sustains[id]?.rollup?.aggregates.find((a) =>
        a.childPath === "finances.liquid.balance",
      );
      if (agg) return agg;
    }
    return undefined;
  });

  return (
    <Split variant="wideLeft">
      {/* ── the graph ────────────────────────────────────────────────────── */}
      <Card
        title="constellation"
        scroll
        right={
          <Meta>
            {telemetry().sustains} sustains · ⊕ {world.holds ? `${world.linked} linked` : "broken"}
          </Meta>
        }
      >
        <Show
          when={layout().root}
          fallback={<Empty>no sustains yet · the store is empty</Empty>}
        >
          {(root) => (
            <svg viewBox="0 0 380 336" class={S.constellationSvg} role="img">
              {/* composition edges — one per real ⊕ link */}
              <For each={layout().kids}>
                {(k) => (
                  <line
                    x1={CENTER.x}
                    y1={CENTER.y}
                    x2={k.x}
                    y2={k.y}
                    stroke={vars.color.border}
                    stroke-width="1"
                  />
                )}
              </For>

              {/* the children */}
              <For each={layout().kids}>
                {(k) => {
                  const n = () => nodeAt(k.id);
                  const tone = () => toneOf(attentionFor(k.id));
                  return (
                    <g class={S.node} onClick={() => props.onOpen(k.id)} role="button" tabindex="0">
                      <circle
                        cx={k.x}
                        cy={k.y}
                        r="26"
                        fill={world.selected === k.id ? vars.color.amberGlow : vars.color.bgRaised}
                        stroke={world.selected === k.id ? vars.color.amber : vars.color.borderMid}
                        stroke-width="1"
                      />
                      <circle cx={k.x + 18} cy={k.y - 18} r="3.5" fill={tone()} />
                      <text
                        x={k.x}
                        y={k.y - 2}
                        text-anchor="middle"
                        class={S.nodeLabel}
                        fill={vars.color.textPrimary}
                      >
                        {n()?.summary.label ?? k.id}
                      </text>
                      <text
                        x={k.x}
                        y={k.y + 11}
                        text-anchor="middle"
                        class={S.nodeFigure}
                        fill={vars.color.textSecondary}
                      >
                        {fmt(n()?.summary.liquid)}
                      </text>
                    </g>
                  );
                }}
              </For>

              {/* the root */}
              <g
                class={S.node}
                onClick={() => props.onOpen(root().summary.id)}
                role="button"
                tabindex="0"
              >
                <circle
                  cx={CENTER.x}
                  cy={CENTER.y}
                  r="40"
                  fill={
                    world.selected === root().summary.id
                      ? vars.color.amberGlow
                      : vars.color.bgOverlay
                  }
                  stroke={
                    world.selected === root().summary.id ? vars.color.amber : vars.color.borderLight
                  }
                  stroke-width="1"
                />
                <circle
                  cx={CENTER.x + 28}
                  cy={CENTER.y - 28}
                  r="4"
                  fill={toneOf(attentionFor(root().summary.id))}
                />
                <text
                  x={CENTER.x}
                  y={CENTER.y - 4}
                  text-anchor="middle"
                  class={S.nodeLabel}
                  fill={vars.color.textPrimary}
                >
                  {root().summary.label}
                </text>
                <text
                  x={CENTER.x}
                  y={CENTER.y + 12}
                  text-anchor="middle"
                  class={S.nodeFigure}
                  fill={vars.color.textSecondary}
                >
                  {fmt(root().summary.liquid)}
                </text>
              </g>
            </svg>
          )}
        </Show>

        {/* ── telemetry strip ────────────────────────────────────────────── */}
        <TelemetryStrip>
          <TelemetryCell label="sustains">{telemetry().sustains}</TelemetryCell>
          <TelemetryCell label="logged events">{telemetry().events}</TelemetryCell>
          <TelemetryCell label="rules watched">{telemetry().rules}</TelemetryCell>
          <TelemetryCell label="rules broken" tone={telemetry().broken > 0 ? "danger" : "ok"}>
            {telemetry().broken}
          </TelemetryCell>
          {/* ★★★ Summed BY THE ENGINE now, not by this screen. `compute_rollup`
              folds the household's own state and every linked child's; this
              cell reads what it returned. ★ Still a dash when nothing was
              readable — `sum` over nothing is 0, and a confident zero for a
              household nobody could read would be the same lie in a new place. */}
          <TelemetryCell
            label="household liquid"
            tone={householdTotal()?.grounded ? undefined : "idle"}
          >
            <Show when={householdTotal()?.grounded} fallback="—">
              {fmt(householdTotal()!.value)}
            </Show>
          </TelemetryCell>
        </TelemetryStrip>
        <Show
          when={householdTotal()}
          fallback={
            <Caption>
              No Sustain here declares a household total. Every figure above is a real count over
              the persisted world.
            </Caption>
          }
        >
          {(a) => (
            <Caption>
              <strong>household liquid is the engine's roll-up ρ</strong> — {a().op.toLowerCase()}{" "}
              over <code>{a().childPath}</code>, folded from the household's own state and{" "}
              {a().included.filter((c) => !c.isHousehold).length} member
              {a().included.filter((c) => !c.isHousehold).length === 1 ? "" : "s"}, fresh on every
              read.
              <Show when={a().excluded.length > 0}>
                {" "}
                {a().excluded.length} contributor{a().excluded.length === 1 ? " was" : "s were"} not
                readable and {a().excluded.length === 1 ? "is" : "are"} left out rather than counted
                as zero — see Composition.
              </Show>
            </Caption>
          )}
        </Show>
      </Card>

      {/* ── right column ─────────────────────────────────────────────────── */}
      <Column>
        {/* the live gate stream */}
        <Card
          title="gate stream · live"
          scroll
          right={
            <>
              <Badge tone="ok">{world.pushes} committed</Badge>
              <Show when={world.refusals > 0}>
                <Badge tone="danger">{world.refusals} refused</Badge>
              </Show>
            </>
          }
        >
          <Show
            when={world.stream.length > 0}
            fallback={
              <Empty>the gate is quiet · nothing has been asked of it since this window opened</Empty>
            }
          >
            <For each={world.stream}>
              {(e) => (
                <div class={e.kind === "refused" ? S.streamRowRefused : S.streamRow}>
                  <span
                    class={S.streamVerdict}
                    style={{ color: e.kind === "refused" ? vars.color.danger : vars.color.teal }}
                  >
                    {e.kind === "refused" ? "REFUSED" : "ADMITTED"}
                  </span>
                  <Fill>
                    <Value>{e.operator}</Value>{" "}
                    <Meta>{world.sustains[e.sustainId]?.summary.label ?? e.sustainId}</Meta>
                    <Show when={e.kind === "admitted"}>
                      <Caption>
                        #{(e as { seq: number }).seq} ·{" "}
                        {(e as { events: string[] }).events.join(", ") || "no events"}
                      </Caption>
                    </Show>
                    <Show when={e.kind === "refused"}>
                      <div class={S.caption} style={{ color: vars.color.danger }}>
                        {(e as { reason: string }).reason || (e as { rule: string }).rule}
                      </div>
                      <Caption>nothing changed · nothing logged</Caption>
                    </Show>
                  </Fill>
                </div>
              )}
            </For>
          </Show>
        </Card>

        {/* needs attention, across the household */}
        <Card title="needs attention · household" right={<Meta>{attention().length}</Meta>}>
          <Show
            when={attention().length > 0}
            fallback={<Empty>nothing needs you · every rule holds</Empty>}
          >
            <For each={attention()}>
              {(a) => (
                <Row onClick={() => props.onOpen(a.sustainId)}>
                  <NoteRow tone={a.severity === "danger" ? "danger" : "warn"}>
                    <Fill>
                      <Value>{a.what}</Value>{" "}
                      <Meta>
                        {a.kind} · {a.label}
                      </Meta>
                      <Caption>{a.why}</Caption>
                    </Fill>
                  </NoteRow>
                </Row>
              )}
            </For>
          </Show>
          <Note>
            <Caption>
              A broken rule is the engine's verdict. The {Math.round(ATTENTION_AT * 100)}% pocket
              line is this app's declared policy over real numbers.
            </Caption>
          </Note>
        </Card>
      </Column>
    </Split>
  );
}
