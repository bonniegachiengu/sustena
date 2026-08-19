/**
 * MONITOR — live on the push channel.
 *
 * ★★★ Nothing here polls and nothing re-fetches on change. Every value reads
 * from the live store, which is filled by `events.committed`. When a pocket
 * moves, the cells bound to that pocket repaint and the rest of the screen does
 * not — that is Solid's fine-grained reactivity doing the work a virtual DOM
 * would otherwise redo.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import * as s from "../styles/app.css";
import { vars } from "../styles/tokens.css";
import { fmt, pockets, type Pocket } from "../lib/engine";
import { childrenOf, selectedSustain, world } from "../lib/live";

/**
 * ★★ The attention threshold is a **UI policy, declared here**, not an engine
 * reading. The engine has a real urgency notion — `d(s,V)` over a declared
 * `Region` — but these templates declare invariants rather than intervals, and
 * inventing a Region to borrow its authority would be declaring thresholds
 * nobody chose. So: a stated rule over real numbers, labelled as ours.
 */
const ATTENTION_AT = 0.8;

type Sort = "urgency" | "balance";

const pct = (p: Pocket): number | null =>
  p.allocated > 0 ? p.spent / p.allocated : null;

function meterColour(fraction: number | null): string {
  if (fraction === null) return vars.color.borderLight;
  if (fraction >= 1) return vars.color.danger;
  if (fraction >= ATTENTION_AT) return vars.color.warn;
  return vars.color.teal;
}

export default function Monitor() {
  const [sort, setSort] = createSignal<Sort>("urgency");

  const live = () => selectedSustain();
  const rows = createMemo(() => {
    const list = pockets(live()?.state);
    const by = sort();
    return [...list].sort((a, b) => {
      if (by === "urgency") {
        // ★ An unmeasurable pocket (nothing allocated) sorts last rather than
        //   as calm — a null is not a zero.
        const pa = pct(a);
        const pb = pct(b);
        if (pa === null && pb === null) return a.name.localeCompare(b.name);
        if (pa === null) return 1;
        if (pb === null) return -1;
        return pb - pa;
      }
      return b.left - a.left;
    });
  });

  /** Real signals only: pockets past the declared threshold, and rules the engine says are broken. */
  const attention = createMemo(() => {
    const out: { kind: string; what: string; why: string; tone: string }[] = [];
    for (const c of live()?.constraints ?? []) {
      if (!c.holds) {
        out.push({
          kind: "rule",
          what: c.id,
          why: c.reason || c.expression,
          tone: vars.color.danger,
        });
      }
    }
    for (const p of pockets(live()?.state)) {
      const f = pct(p);
      if (f !== null && f >= ATTENTION_AT) {
        out.push({
          kind: "pocket",
          what: p.name,
          why: `${Math.round(f * 100)}% spent · ${fmt(p.left)} of ${fmt(p.allocated)} left`,
          tone: f >= 1 ? vars.color.danger : vars.color.warn,
        });
      }
    }
    return out;
  });

  const kids = createMemo(() => (live() ? childrenOf(live()!.summary.id) : []));

  return (
    <div class={s.main}>
      {/* ── left ─────────────────────────────────────────────────────────── */}
      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>live state</span>
            <span class={s.spacer} />
            <div class={s.tabs}>
              <button
                class={sort() === "urgency" ? s.tabActive : s.tab}
                onClick={() => setSort("urgency")}
              >
                urgency
              </button>
              <button
                class={sort() === "balance" ? s.tabActive : s.tab}
                onClick={() => setSort("balance")}
              >
                balance
              </button>
            </div>
          </header>
          <div class={s.cardBody}>
            <Show when={live()} fallback={<div class={s.empty}>nothing selected</div>}>
              {(l) => (
                <>
                  <div class={s.row}>
                    <span class={s.label}>liquid balance</span>
                    <span class={s.valueBig}>{fmt(l().summary.liquid)}</span>
                  </div>
                  <Show
                    when={rows().length > 0}
                    fallback={<div class={s.empty}>no pockets yet</div>}
                  >
                    <For each={rows()}>
                      {(p) => {
                        const f = () => pct(p);
                        return (
                          <div class={s.pocketBlock}>
                            <div class={s.pocketHead}>
                              <span class={s.value}>{p.name}</span>
                              <span class={s.mono}>
                                {f() === null ? "—" : `${Math.round(f()! * 100)}%`}
                              </span>
                              <span class={s.value}>{fmt(p.left)}</span>
                            </div>
                            <div class={s.meter}>
                              <div
                                class={s.meterFill}
                                style={{
                                  width: `${Math.min(100, (f() ?? 0) * 100)}%`,
                                  background: meterColour(f()),
                                }}
                              />
                            </div>
                            <div class={s.attentionWhy}>
                              spent {fmt(p.spent)} of {fmt(p.allocated)}
                            </div>
                          </div>
                        );
                      }}
                    </For>
                  </Show>
                </>
              )}
            </Show>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>needs attention</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>{attention().length}</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={attention().length > 0}
              fallback={
                <div class={s.empty}>nothing needs you · all rules hold, no pocket is strained</div>
              }
            >
              <For each={attention()}>
                {(a) => (
                  <div class={s.attentionRow}>
                    <span class={s.dot} style={{ background: a.tone, "margin-top": "5px" }} />
                    <span>
                      <span class={s.value}>{a.what}</span>{" "}
                      <span class={s.sustainMeta}>{a.kind}</span>
                      <br />
                      <span class={s.attentionWhy}>{a.why}</span>
                    </span>
                  </div>
                )}
              </For>
            </Show>
            <div class={s.attentionWhy} style={{ "margin-top": vars.space.md }}>
              A broken rule is the engine's own verdict. The {Math.round(ATTENTION_AT * 100)}%
              pocket threshold is this app's declared policy over real numbers — not an engine
              reading, and labelled so.
            </div>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>⊕ · roll-up</span>
          </header>
          <div class={s.cardBody}>
            <div class={s.mono}>
              {kids().length} linked {kids().length === 1 ? "child" : "children"}
            </div>
            <Show when={!world.rollupAvailable}>
              <div class={s.unavailable} style={{ "margin-top": vars.space.sm }}>
                <span class={s.unavailableTitle}>roll-up ρ · not available</span>
                The engine holds and checks this tree but cannot yet fold a child's state into a
                parent aggregate — that exists in the Python engine and is not ported to
                <code> sustena-core</code>. Nothing is summed here.
              </div>
            </Show>
          </div>
        </section>
      </div>

      {/* ── right ────────────────────────────────────────────────────────── */}
      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>V · the engine's verdict, now</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={(live()?.constraints ?? []).length > 0}
              fallback={<div class={s.empty}>no invariants declared</div>}
            >
              <For each={live()!.constraints}>
                {(c) => (
                  <div class={s.invariantRow}>
                    <span
                      class={s.dot}
                      style={{
                        background: c.holds ? vars.color.teal : vars.color.danger,
                        "margin-top": "5px",
                      }}
                    />
                    <span>
                      <strong style={{ color: vars.color.textPrimary }}>{c.id}</strong>{" "}
                      <span class={s.sustainMeta}>{c.holds ? "holds" : "broken"}</span>
                      <br />
                      {c.expression}
                      <Show when={!c.holds && c.reason}>
                        <br />
                        <span style={{ color: vars.color.danger }}>{c.reason}</span>
                      </Show>
                    </span>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>the event log · state = fold(this)</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>{live()?.log.length ?? 0} entries</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={(live()?.log ?? []).length > 0}
              fallback={<div class={s.empty}>no log yet</div>}
            >
              <For each={[...(live()?.log ?? [])].reverse()}>
                {(e) => (
                  <div class={s.logRow}>
                    <span class={s.seqCell}>#{e.seq}</span>
                    <span>
                      <span style={{ color: vars.color.textPrimary }}>{e.operator}</span>
                      <Show when={e.events.length > 0}>
                        <br />
                        <For each={e.events}>{(ev) => <span>↳ {ev.name} </span>}</For>
                      </Show>
                    </span>
                    <span class={s.seqCell}>
                      {e.mutations} mut{e.mutations === 1 ? "" : "s"}
                    </span>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </section>
      </div>
    </div>
  );
}
