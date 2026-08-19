/**
 * CONSOLE — ask the gate.
 *
 * ★ The one screen where a refusal belongs. The live store never carries one
 * (the host emits nothing for a change that did not happen), so the verdict is
 * held here, locally, as the answer to *this* request.
 */
import { createSignal, For, Show } from "solid-js";
import * as s from "../styles/app.css";
import { vars } from "../styles/tokens.css";
import { engine, fmt, liquidBalance, pockets, type GateResult, type JsonValue } from "../lib/engine";
import { selectedSustain, world } from "../lib/live";

const PRESETS: { label: string; operator: string; params: Record<string, unknown> }[] = [
  { label: "income 4500", operator: "budget.record_income", params: { amount: 4500, source: "salary" } },
  { label: "add pocket", operator: "budget.add_pocket", params: { pocket_name: "food" } },
  { label: "allocate 1200 → food", operator: "budget.allocate", params: { pocket_name: "food", amount: 1200 } },
  { label: "spend 340 ← food", operator: "budget.spend", params: { pocket_name: "food", amount: 340 } },
  { label: "overspend 9000 ← food", operator: "budget.spend", params: { pocket_name: "food", amount: 9000 } },
];

function Verdict(props: { result: GateResult }) {
  const v = () => props.result.verdict;
  const box = () =>
    v() === "admitted" ? s.verdictAdmitted : v() === "refused" ? s.verdictRefused : s.verdictDeferred;
  const badge = () =>
    v() === "admitted" ? s.badgeAdmitted : v() === "refused" ? s.badgeRefused : s.badgeDeferred;
  const word = () => (v() === "admitted" ? "ADMITTED" : v() === "refused" ? "REFUSED" : "DEFERRED");

  return (
    <div class={box()}>
      <div style={{ display: "flex", "align-items": "baseline", gap: vars.space.md }}>
        <span class={badge()}>{word()}</span>
        <span class={s.mono}>{props.result.operator}</span>
      </div>
      <Show when={props.result.reason}>{(r) => <p class={s.reason}>{r()}</p>}</Show>
      <Show when={props.result.constraintViolated}>
        {(c) => <div class={s.reasonCode}>rule · {c()}</div>}
      </Show>
      <div class={s.reasonCode}>
        {props.result.mutations} mutation{props.result.mutations === 1 ? "" : "s"} ·{" "}
        {props.result.events.length} event{props.result.events.length === 1 ? "" : "s"}
        <Show when={v() === "refused"}> · state unchanged · nothing logged · nothing pushed</Show>
        <Show when={v() === "admitted"}> · appended to the log · pushed to every live screen</Show>
      </div>
    </div>
  );
}

export default function Console() {
  const [last, setLast] = createSignal<GateResult | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [failure, setFailure] = createSignal<string | null>(null);
  const [operator, setOperator] = createSignal(PRESETS[0]!.operator);
  const [paramsText, setParamsText] = createSignal(JSON.stringify(PRESETS[0]!.params));

  const live = () => selectedSustain();

  const execute = async () => {
    setFailure(null);
    const target = world.selected;
    if (!target) {
      setFailure("no Sustain selected");
      return;
    }
    let parsed: JsonValue;
    try {
      parsed = JSON.parse(paramsText()) as JsonValue;
    } catch (e) {
      setFailure(`params are not valid JSON — ${String(e)}`);
      return;
    }
    setBusy(true);
    try {
      const r = await engine.run(target, operator(), parsed);
      if (r === null) setFailure(`the engine has no Sustain called "${target}"`);
      else setLast(r);
      // ★★★ NO REFETCH. A committed call arrives at every live screen through
      //   the push channel; asking again here would be the polling this slice
      //   exists to remove — and would hide a broken channel behind a re-read.
    } catch (e) {
      setFailure(`the engine call itself failed — ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class={s.main}>
      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>Σ · the selected Sustain</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>{live()?.summary.id ?? "—"}</span>
          </header>
          <div class={s.cardBody}>
            <Show when={live()} fallback={<div class={s.empty}>nothing selected</div>}>
              {(l) => (
                <>
                  <div class={s.row}>
                    <span class={s.label}>liquid balance</span>
                    <span class={s.valueBig}>{fmt(liquidBalance(l().state))}</span>
                  </div>
                  <Show when={pockets(l().state).length > 0}>
                    <For each={pockets(l().state)}>
                      {(p) => (
                        <div class={s.pocketRow}>
                          <span class={s.value}>{p.name}</span>
                          <span class={s.mono}>alloc {fmt(p.allocated)}</span>
                          <span class={s.mono}>spent {fmt(p.spent)}</span>
                          <span class={s.value}>left {fmt(p.left)}</span>
                        </div>
                      )}
                    </For>
                  </Show>
                </>
              )}
            </Show>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>S · state, as the engine holds it</span>
          </header>
          <div class={s.cardBody}>
            <Show when={live()?.state} fallback={<div class={s.empty}>—</div>}>
              <pre class={s.codeBlock}>{JSON.stringify(live()!.state, null, 2)}</pre>
            </Show>
          </div>
        </section>
      </div>

      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>console · ask the gate</span>
          </header>
          <div class={s.cardBody}>
            <div class={s.presetRow} style={{ "margin-bottom": vars.space.md }}>
              <For each={PRESETS}>
                {(p) => (
                  <button
                    class={s.chip}
                    onClick={() => {
                      setOperator(p.operator);
                      setParamsText(JSON.stringify(p.params));
                    }}
                  >
                    {p.label}
                  </button>
                )}
              </For>
            </div>

            <div class={s.field}>
              <label class={s.label} for="op">
                operator · targets {live()?.summary.label ?? "nothing"}
              </label>
              <input
                id="op"
                class={s.input}
                value={operator()}
                onInput={(e) => setOperator(e.currentTarget.value)}
              />
            </div>

            <div class={s.field} style={{ "margin-top": vars.space.md }}>
              <label class={s.label} for="params">
                params (json)
              </label>
              <input
                id="params"
                class={s.input}
                value={paramsText()}
                onInput={(e) => setParamsText(e.currentTarget.value)}
              />
            </div>

            <div style={{ "margin-top": vars.space.lg }}>
              <button class={s.button} onClick={execute} disabled={busy() || !world.selected}>
                {busy() ? "asking…" : "execute"}
              </button>
            </div>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>the gate's verdict</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={last()}
              fallback={<div class={s.empty}>nothing asked yet · the gate is quiet</div>}
            >
              {(r) => <Verdict result={r()} />}
            </Show>
            <Show when={failure()}>
              {(f) => (
                <div class={s.errorBox} style={{ "margin-top": vars.space.md }}>
                  {f()}
                </div>
              )}
            </Show>
          </div>
        </section>
      </div>
    </div>
  );
}
