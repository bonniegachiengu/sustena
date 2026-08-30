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
import { createMemo, For, Show, type JSX } from "solid-js";
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
import { openRow } from "../lib/nav";
import { ATTENTION_AT, attentionAcross, attentionFor, world } from "../lib/live";

/**
 * ★★★ **The ring GROWS with the ring.** A fixed radius is the bug this screen
 * shipped with: eight households on a circle sized for four put their labels on
 * top of each other and pushed the lowest node out of the frame. Circumference
 * is what has to hold them, so the radius follows the count rather than the
 * other way round.
 */
const NODE_R = 26;
const ROOT_R = 40;
/**
 * How wide a label paints, near enough to bound it.
 *
 * ★★ An estimate, and deliberately a GENEROUS one. It is used only to decide
 * how much room to leave, so reading it high costs a little margin and reading
 * it low costs a clipped name — the two errors are not the same size, and the
 * cheap one is the one to make.
 */
const labelHalfWidth = (text: string) => Math.max(NODE_R, text.length * 3.4);

/** Clear space between two neighbours on the ring. */
const NODE_GAP = 16;
const MIN_RADIUS = 96;

/**
 * A ring wide enough that no two neighbours touch.
 *
 * ★★ Sized from the WIDEST LABEL rather than a constant, because the thing
 * that collides is the name, not the disc. A household of eight short names
 * needs less room than a household of four long ones, and a constant per-node
 * arc is wrong for both. The chord between neighbours is `2·R·sin(π/n)`, so
 * solving it for the radius is the whole calculation.
 */
const ringRadius = (labels: string[]) => {
  const n = Math.max(labels.length, 1);
  const widest = labels.reduce((w, l) => Math.max(w, labelHalfWidth(l) * 2), NODE_R * 2);
  const needed = n === 1 ? 0 : (widest + NODE_GAP) / (2 * Math.sin(Math.PI / n));
  return Math.max(MIN_RADIUS, ROOT_R + NODE_R + 24, needed);
};


type Node = { id: string; label: string; x: number; y: number };

/**
 * One line of the gate stream, navigable when there is somewhere to go.
 *
 * ★★ Same contract as `Row` and `NoteRow`: the handler decides both the
 * behaviour and the appearance, so a line cannot look live and be dead.
 */
function StreamRow(props: {
  refused: boolean;
  onClick?: () => void;
  children: JSX.Element;
}) {
  const cls = () =>
    `${props.refused ? S.streamRowRefused : S.streamRow}${props.onClick ? ` ${S.noteRowButton}` : ""}`;
  return props.onClick ? (
    <button class={cls()} onClick={props.onClick}>
      {props.children}
    </button>
  ) : (
    <div class={cls()}>{props.children}</div>
  );
}

const toneOf = (t: "ok" | "warn" | "danger") =>
  t === "danger" ? vars.color.danger : t === "warn" ? vars.color.warn : vars.color.teal;

export default function Constellation(props: { onOpen: (id: string) => void }) {
  const roots = createMemo(() =>
    world.order.map((k) => world.sustains[k]!).filter((x) => x && x.summary.parent === null),
  );

  /**
   * The graph: one root at the centre, its children on a ring — and a viewBox
   * measured from what was actually drawn.
   *
   * ★★★ **The frame is derived, not declared.** The screen used to carry a
   * literal `viewBox="0 0 380 336"` while placing nodes at a fixed radius, so
   * the drawing and the frame were two independent guesses that agreed for
   * four households and disagreed for eight. Bonnie's bottom node ("Mum")
   * ended up outside the box and under the status bar. Measuring the content
   * and sizing the frame to it makes "everything fits" a property of the
   * layout rather than a coincidence of the numbers.
   */
  const layout = createMemo(() => {
    const root = roots()[0];
    if (!root) {
      return { root: null, kids: [] as Node[], box: "0 0 380 336", center: { x: 190, y: 168 } };
    }
    const kids = world.order
      .map((k) => world.sustains[k]!)
      .filter((x) => x && x.summary.parent === root.summary.id);

    const radius = ringRadius(kids.map((k) => k.summary.label ?? k.summary.id));
    const center = { x: 0, y: 0 };
    const step = (Math.PI * 2) / Math.max(kids.length, 1);

    const placed: Node[] = kids.map((k, i) => ({
      id: k.summary.id,
      label: k.summary.label ?? k.summary.id,
      x: center.x + radius * Math.cos(i * step - Math.PI / 2),
      y: center.y + radius * Math.sin(i * step - Math.PI / 2),
    }));

    // ★★ Bounds over everything that PAINTS, not just the circles: the label
    //    and the figure sit inside the node, but a long name is wider than the
    //    disc that holds it, and the attention dot sits outside it.
    let minX = center.x - ROOT_R;
    let maxX = center.x + ROOT_R;
    let minY = center.y - ROOT_R;
    let maxY = center.y + ROOT_R;
    const grow = (x: number, y: number, halfW: number, halfH: number) => {
      minX = Math.min(minX, x - halfW);
      maxX = Math.max(maxX, x + halfW);
      minY = Math.min(minY, y - halfH);
      maxY = Math.max(maxY, y + halfH);
    };
    grow(center.x, center.y, labelHalfWidth(root.summary.label ?? ""), ROOT_R + 4);
    for (const n of placed) grow(n.x, n.y, labelHalfWidth(n.label), NODE_R + 4);

    const pad = 10;
    const box = [
      (minX - pad).toFixed(1),
      (minY - pad).toFixed(1),
      (maxX - minX + pad * 2).toFixed(1),
      (maxY - minY + pad * 2).toFixed(1),
    ].join(" ");

    return { root, kids: placed, box, center };
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
            <svg
              viewBox={layout().box}
              preserveAspectRatio="xMidYMid meet"
              class={S.constellationSvg}
              role="img"
            >
              {/* composition edges — one per real ⊕ link */}
              <For each={layout().kids}>
                {(k) => (
                  <line
                    x1={layout().center.x}
                    y1={layout().center.y}
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
                        {k.label}
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
                  cx={layout().center.x}
                  cy={layout().center.y}
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
                  cx={layout().center.x + 28}
                  cy={layout().center.y - 28}
                  r="4"
                  fill={toneOf(attentionFor(root().summary.id))}
                />
                <text
                  x={layout().center.x}
                  y={layout().center.y - 4}
                  text-anchor="middle"
                  class={S.nodeLabel}
                  fill={vars.color.textPrimary}
                >
                  {root().summary.label}
                </text>
                <text
                  x={layout().center.x}
                  y={layout().center.y + 12}
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
              Household liquid is the {a().op.toLowerCase()} of <code>{a().childPath}</code>
              {" "}across the household and{" "}
              {a().included.filter((c) => !c.isHousehold).length} member
              {a().included.filter((c) => !c.isHousehold).length === 1 ? "" : "s"}, fresh on every
              read.
              <Show when={a().excluded.length > 0}>
                {" "}
                {a().excluded.length} could not be read and{" "}
                {a().excluded.length === 1 ? "is" : "are"} left out of the total. See Composition.
              </Show>
            </Caption>
          )}
        </Show>
      </Card>

      {/* ── right column ─────────────────────────────────────────────────── */}
      <Column>
        {/* the live gate stream */}
        <Card
          title="activity · live"
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
                /* ★★★ Every line names the Sustain it happened to, and none of
                   them could take you there. A stream that shows a refusal in
                   Cira's habitat and leaves you to find Cira yourself is a
                   notification board, not a cockpit. `openRow` returns nothing
                   when the store has no such Sustain — a line about one that
                   has since been dissolved stays a fact rather than a door. */
                <StreamRow onClick={openRow(e.sustainId)} refused={e.kind === "refused"}>
                  <span
                    class={S.streamVerdict}
                    style={{
                      color:
                        e.kind === "refused"
                          ? vars.color.danger
                          : e.kind === "arrived"
                            ? vars.color.amber
                            : vars.color.teal,
                    }}
                  >
                    {e.kind === "refused"
                      ? "REFUSED"
                      : e.kind === "arrived"
                        ? "ARRIVED"
                        : "ADMITTED"}
                  </span>
                  <Fill>
                    {/* ★ An arrival has no operator: nothing local caused it. */}
                    <Value>
                      {e.kind === "arrived"
                        ? `${e.entries} entrie(s) from a peer`
                        : e.operator}
                    </Value>{" "}
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
                </StreamRow>
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
