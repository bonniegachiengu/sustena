/**
 * SIMULATE — a fork through the real gate.
 *
 * ★★★ STEP-0 found **no scenario-fork API in `sustena-core`** (`ensemble::Scenario`
 * is about model ensembles, not Sustain simulation). So a branch here is exactly
 * two things: `state.clone()` in the host, and the **same `execute_admitted`**
 * every real call goes through. It is the engine's own gate on a copy — not a
 * simulator this app invented — so a step refused here is refused for real, with
 * the same reason.
 *
 * ★★ **Nothing is written.** No log, no ledger, no meter, no cached state. A
 * branch is a value.
 */
import { createSignal, For, Show } from "solid-js";
import * as s from "../styles/app.css";
import { vars } from "../styles/tokens.css";
import { engine, fmt, liquidBalance, pockets, type Branch, type JsonValue } from "../lib/engine";
import { selectedSustain, world } from "../lib/live";

type Step = { operator: string; params: string };

const START: Step[] = [
  { operator: "budget.record_income", params: '{"amount":10000,"source":"hypothetical"}' },
];

/** A plain-language diff between two engine states. */
function diff(before: unknown, after: unknown): { path: string; from: string; to: string }[] {
  const flat = (v: unknown, prefix = ""): Record<string, string> => {
    if (v === null || typeof v !== "object") return { [prefix]: String(v) };
    let out: Record<string, string> = {};
    for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
      out = { ...out, ...flat(val, prefix ? `${prefix}.${k}` : k) };
    }
    return out;
  };
  const a = flat(before);
  const b = flat(after);
  const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
  const rows: { path: string; from: string; to: string }[] = [];
  for (const k of keys) {
    if (a[k] !== b[k]) rows.push({ path: k, from: a[k] ?? "—", to: b[k] ?? "—" });
  }
  return rows.sort((x, y) => x.path.localeCompare(y.path));
}

export default function Simulate() {
  const [steps, setSteps] = createSignal<Step[]>(START);
  const [branch, setBranch] = createSignal<Branch | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [failure, setFailure] = createSignal<string | null>(null);
  const [promoted, setPromoted] = createSignal<string | null>(null);

  const live = () => selectedSustain();

  const run = async () => {
    const target = world.selected;
    if (!target) return;
    setFailure(null);
    setPromoted(null);
    let parsed: [string, JsonValue][];
    try {
      parsed = steps().map((st) => [st.operator, JSON.parse(st.params) as JsonValue]);
    } catch (e) {
      setFailure(`params are not valid JSON — ${String(e)}`);
      return;
    }
    setBusy(true);
    try {
      setBranch(await engine.simulate(target, parsed));
    } catch (e) {
      setFailure(String(e));
    } finally {
      setBusy(false);
    }
  };

  /**
   * ★★★ Promote — replay the branch through the REAL `run_operator`.
   *
   * It does **not** trust the simulation. Every step is re-decided against
   * current live state, so a step that passed in the fork can honestly refuse
   * here if the world moved in between. That is the correct outcome, not a bug.
   */
  const promote = async () => {
    const target = world.selected;
    const b = branch();
    if (!target || !b) return;
    setBusy(true);
    setFailure(null);
    try {
      const done: string[] = [];
      for (const st of steps()) {
        const r = await engine.run(target, st.operator, JSON.parse(st.params) as JsonValue);
        if (!r) {
          setPromoted(`stopped: no Sustain "${target}"`);
          return;
        }
        if (r.verdict !== "admitted") {
          setPromoted(
            `stopped at ${st.operator} after ${done.length} step(s) — REFUSED: ${r.reason ?? r.constraintViolated ?? "no reason"}`,
          );
          return;
        }
        done.push(st.operator);
      }
      setPromoted(`promoted — ${done.length} step(s) committed for real`);
      setBranch(null);
    } catch (e) {
      setFailure(String(e));
    } finally {
      setBusy(false);
    }
  };

  const finalState = () => {
    const b = branch();
    if (!b || b.steps.length === 0) return null;
    return b.steps[b.steps.length - 1]!.state;
  };

  return (
    <div class={s.main}>
      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>branch · from live state</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>{live()?.summary.label ?? "—"}</span>
          </header>
          <div class={s.cardBody}>
            <div class={s.hypothetical} style={{ "margin-bottom": vars.space.md }}>
              <span class={s.unavailableTitle}>◆ hypothetical</span>
              A branch runs against a <strong>copy</strong> of live state, through the same gate a
              real call uses. Nothing is written — no log, no ledger, no meter. Promoting replays
              it for real, and can still be refused if the world moved.
            </div>

            <For each={steps()}>
              {(st, i) => (
                <div class={s.field} style={{ "margin-bottom": vars.space.md }}>
                  <label class={s.label}>step {i() + 1}</label>
                  <input
                    class={s.input}
                    value={st.operator}
                    onInput={(e) =>
                      setSteps((v) =>
                        v.map((x, j) => (j === i() ? { ...x, operator: e.currentTarget.value } : x)),
                      )
                    }
                  />
                  <input
                    class={s.input}
                    value={st.params}
                    onInput={(e) =>
                      setSteps((v) =>
                        v.map((x, j) => (j === i() ? { ...x, params: e.currentTarget.value } : x)),
                      )
                    }
                  />
                </div>
              )}
            </For>

            <div class={s.presetRow}>
              <button
                class={s.chip}
                onClick={() => setSteps((v) => [...v, { operator: "budget.allocate", params: '{"pocket_name":"food","amount":500}' }])}
              >
                + step
              </button>
              <Show when={steps().length > 1}>
                <button class={s.chip} onClick={() => setSteps((v) => v.slice(0, -1))}>
                  − step
                </button>
              </Show>
            </div>

            <div style={{ "margin-top": vars.space.lg, display: "flex", gap: vars.space.sm }}>
              <button class={s.button} onClick={run} disabled={busy() || !world.selected}>
                {busy() ? "forking…" : "run branch"}
              </button>
              <Show when={branch()}>
                <button class={s.buttonGhost} onClick={promote} disabled={busy()}>
                  promote to reality
                </button>
              </Show>
            </div>

            <Show when={failure()}>
              {(f) => <div class={s.errorBox} style={{ "margin-top": vars.space.md }}>{f()}</div>}
            </Show>
            <Show when={promoted()}>
              {(p) => (
                <div class={s.verdictAdmitted} style={{ "margin-top": vars.space.md }}>
                  <span class={s.badgeAdmitted}>PROMOTED</span>
                  <p class={s.reason}>{p()}</p>
                </div>
              )}
            </Show>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>⊕ · roll-up, hypothetical</span>
          </header>
          <div class={s.cardBody}>
            {/* ★ Same honest state as everywhere else. A hypothetical roll-up
                would need ρ just as much as a real one does. */}
            <div class={s.unavailable}>
              <span class={s.unavailableTitle}>roll-up ρ · not available</span>
              A branch cannot show a hypothetical household aggregate for the same reason the
              Monitor cannot show a real one: ρ is not in <code>sustena-core</code>. Nothing is
              summed, in reality or in a fork.
            </div>
          </div>
        </section>
      </div>

      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>the branch</span>
            <span class={s.spacer} />
            <Show when={branch()}>
              <span class={s.hypotheticalBadge}>◆ nothing written</span>
            </Show>
          </header>
          <div class={s.cardBody}>
            <Show
              when={branch()}
              fallback={<div class={s.empty}>no branch yet · run one to see what would happen</div>}
            >
              {(b) => (
                <>
                  <For each={b().steps}>
                    {(st, i) => (
                      <div class={st.verdict === "refused" ? s.verdictRefused : s.verdictAdmitted} style={{ "margin-bottom": vars.space.sm }}>
                        <div style={{ display: "flex", gap: vars.space.md, "align-items": "baseline" }}>
                          <span class={st.verdict === "refused" ? s.badgeRefused : s.badgeAdmitted}>
                            {st.verdict.toUpperCase()}
                          </span>
                          <span class={s.mono}>
                            {i() + 1}. {st.operator}
                          </span>
                        </div>
                        <Show when={st.reason}>{(r) => <p class={s.reason}>{r()}</p>}</Show>
                        <div class={s.reasonCode}>
                          {st.mutations} mutation{st.mutations === 1 ? "" : "s"} ·{" "}
                          {st.events.length} event{st.events.length === 1 ? "" : "s"}
                          <Show when={st.verdict === "refused"}> · the branch stops here</Show>
                        </div>
                      </div>
                    )}
                  </For>

                  <Show when={finalState()}>
                    {(fin) => (
                      <>
                        <div class={s.label} style={{ "margin-top": vars.space.lg }}>
                          state diff · live → hypothetical
                        </div>
                        <For
                          each={diff(b().from, fin())}
                          fallback={<div class={s.empty}>nothing moved</div>}
                        >
                          {(d) => (
                            <div class={s.diffRow}>
                              <span class={s.mono}>{d.path}</span>
                              <span class={s.seqCell}>{d.from}</span>
                              <span class={s.value}>→ {d.to}</span>
                            </div>
                          )}
                        </For>

                        <div class={s.row} style={{ "margin-top": vars.space.md }}>
                          <span class={s.label}>liquid · hypothetical</span>
                          <span class={s.valueBig}>{fmt(liquidBalance(fin()))}</span>
                        </div>
                        <For each={pockets(fin())}>
                          {(p) => (
                            <div class={s.pocketRow}>
                              <span class={s.value}>{p.name}</span>
                              <span class={s.mono}>alloc {fmt(p.allocated)}</span>
                              <span class={s.mono}>spent {fmt(p.spent)}</span>
                              <span class={s.value}>left {fmt(p.left)}</span>
                            </div>
                          )}
                        </For>
                      </>
                    )}
                  </Show>
                </>
              )}
            </Show>
          </div>
        </section>
      </div>
    </div>
  );
}
