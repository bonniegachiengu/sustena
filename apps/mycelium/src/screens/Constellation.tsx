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
import * as s from "../styles/app.css";
import { vars } from "../styles/tokens.css";
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
    const broken = all.reduce(
      (n, x) => n + x.summary.constraints.filter((c) => !c.holds).length,
      0,
    );
    return { sustains: all.length, events, rules, broken };
  });

  const attention = createMemo(attentionAcross);

  const nodeAt = (id: string) => world.sustains[id];

  return (
    <div class={s.constellationGrid}>
      {/* ── the graph ─────────────────────────────────────────────────────── */}
      <section class={s.card} style={{ "min-height": 0, display: "flex", "flex-direction": "column" }}>
        <header class={s.cardHead}>
          <span class={s.cardTitle}>constellation</span>
          <span class={s.spacer} />
          <span class={s.sustainMeta}>
            {telemetry().sustains} sustains · ⊕ {world.holds ? `${world.linked} linked` : "broken"}
          </span>
        </header>
        <div class={s.cardBody} style={{ "overflow-y": "auto", "min-height": 0 }}>
          <Show
            when={layout().root}
            fallback={<div class={s.empty}>no sustains yet · the store is empty</div>}
          >
            {(root) => (
              <svg viewBox="0 0 380 336" class={s.constellationSvg} role="img">
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
                      <g
                        class={s.node}
                        onClick={() => props.onOpen(k.id)}
                        role="button"
                        tabindex="0"
                      >
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
                          class={s.nodeLabel}
                          fill={vars.color.textPrimary}
                        >
                          {n()?.summary.label ?? k.id}
                        </text>
                        <text
                          x={k.x}
                          y={k.y + 11}
                          text-anchor="middle"
                          class={s.nodeFigure}
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
                  class={s.node}
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
                      world.selected === root().summary.id
                        ? vars.color.amber
                        : vars.color.borderLight
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
                    class={s.nodeLabel}
                    fill={vars.color.textPrimary}
                  >
                    {root().summary.label}
                  </text>
                  <text
                    x={CENTER.x}
                    y={CENTER.y + 12}
                    text-anchor="middle"
                    class={s.nodeFigure}
                    fill={vars.color.textSecondary}
                  >
                    {fmt(root().summary.liquid)}
                  </text>
                </g>
              </svg>
            )}
          </Show>

          {/* ── telemetry strip ─────────────────────────────────────────── */}
          <div class={s.telemetry}>
            <div class={s.telemetryCell}>
              <span class={s.label}>sustains</span>
              <span class={s.value}>{telemetry().sustains}</span>
            </div>
            <div class={s.telemetryCell}>
              <span class={s.label}>logged events</span>
              <span class={s.value}>{telemetry().events}</span>
            </div>
            <div class={s.telemetryCell}>
              <span class={s.label}>rules watched</span>
              <span class={s.value}>{telemetry().rules}</span>
            </div>
            <div class={s.telemetryCell}>
              <span class={s.label}>rules broken</span>
              <span
                class={s.value}
                style={{ color: telemetry().broken > 0 ? vars.color.danger : vars.color.teal }}
              >
                {telemetry().broken}
              </span>
            </div>
            <div class={s.telemetryCell}>
              <span class={s.label}>household liquid</span>
              {/* ★★★ NOT summed. Roll-up ρ is not in the core, and adding six
                  numbers here would be host arithmetic wearing an engine's
                  name. The honest figure is no figure. */}
              <span class={s.value} style={{ color: vars.color.textDim }}>
                —
              </span>
            </div>
          </div>
          <div class={s.attentionWhy}>
            <strong>household liquid is not summed.</strong> Roll-up ρ — folding children into a
            parent aggregate — does not exist in <code>sustena-core</code>. Every other figure
            above is a real count over the persisted world.
          </div>
        </div>
      </section>

      {/* ── right column ──────────────────────────────────────────────────── */}
      <div class={s.column}>
        {/* the live gate stream */}
        <section class={s.card} style={{ "min-height": 0, display: "flex", "flex-direction": "column" }}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>gate stream · live</span>
            <span class={s.spacer} />
            <span class={s.pushBadge}>{world.pushes} committed</span>
            <Show when={world.refusals > 0}>
              <span class={s.refusedBadge}>{world.refusals} refused</span>
            </Show>
          </header>
          <div class={s.cardBody} style={{ "overflow-y": "auto", "min-height": 0 }}>
            <Show
              when={world.stream.length > 0}
              fallback={
                <div class={s.empty}>
                  the gate is quiet · nothing has been asked of it since this window opened
                </div>
              }
            >
              <For each={world.stream}>
                {(e) => (
                  <div class={e.kind === "refused" ? s.streamRowRefused : s.streamRow}>
                    <span
                      class={s.streamVerdict}
                      style={{
                        color: e.kind === "refused" ? vars.color.danger : vars.color.teal,
                      }}
                    >
                      {e.kind === "refused" ? "REFUSED" : "ADMITTED"}
                    </span>
                    <span>
                      <span style={{ color: vars.color.textPrimary }}>{e.operator}</span>{" "}
                      <span class={s.sustainMeta}>
                        {world.sustains[e.sustainId]?.summary.label ?? e.sustainId}
                      </span>
                      <Show when={e.kind === "admitted"}>
                        <br />
                        <span class={s.attentionWhy}>
                          #{(e as { seq: number }).seq} ·{" "}
                          {(e as { events: string[] }).events.join(", ") || "no events"}
                        </span>
                      </Show>
                      <Show when={e.kind === "refused"}>
                        <br />
                        <span class={s.attentionWhy} style={{ color: vars.color.danger }}>
                          {(e as { reason: string }).reason || (e as { rule: string }).rule}
                        </span>
                        <br />
                        <span class={s.attentionWhy}>nothing changed · nothing logged</span>
                      </Show>
                    </span>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </section>

        {/* needs attention, across the household */}
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>needs attention · household</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>{attention().length}</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={attention().length > 0}
              fallback={<div class={s.empty}>nothing needs you · every rule holds</div>}
            >
              <For each={attention()}>
                {(a) => (
                  <button class={s.attentionButton} onClick={() => props.onOpen(a.sustainId)}>
                    <span
                      class={s.dot}
                      style={{
                        background:
                          a.severity === "danger" ? vars.color.danger : vars.color.warn,
                        "margin-top": "5px",
                      }}
                    />
                    <span>
                      <span class={s.value}>{a.what}</span>{" "}
                      <span class={s.sustainMeta}>
                        {a.kind} · {a.label}
                      </span>
                      <br />
                      <span class={s.attentionWhy}>{a.why}</span>
                    </span>
                  </button>
                )}
              </For>
            </Show>
            <div class={s.attentionWhy} style={{ "margin-top": vars.space.md }}>
              A broken rule is the engine's verdict. The {Math.round(ATTENTION_AT * 100)}% pocket
              line is this app's declared policy over real numbers.
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
