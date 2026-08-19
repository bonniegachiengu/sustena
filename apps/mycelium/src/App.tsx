/**
 * MYCELIUM — walking skeleton.
 *
 * One screen, and it is the **Console gate result**: ask the engine to do
 * something, and render what the gate decided. That is the screen worth
 * building first because it is the only one that fails honestly — a Monitor
 * showing state proves the read path, but the gate proves the whole
 * contract: a refusal has to arrive as a refusal, with the engine's own reason,
 * and the state has to be visibly unchanged behind it.
 */
import { createResource, createSignal, For, Show, type JSX } from "solid-js";
import * as s from "./styles/app.css";
import { vars } from "./styles/tokens.css";
import { engine, fmt, liquidBalance, pockets, type GateResult, type JsonValue } from "./lib/engine";

/** The three moves the demo household actually offers, with sane starting args. */
const PRESETS: { label: string; operator: string; params: Record<string, unknown> }[] = [
  { label: "income 4500", operator: "budget.record_income", params: { amount: 4500, source: "salary" } },
  { label: "allocate 1200 → food", operator: "budget.allocate", params: { pocket_name: "food", amount: 1200 } },
  { label: "spend 340 ← food", operator: "budget.spend", params: { pocket_name: "food", amount: 340 } },
  { label: "overspend 9000 ← food", operator: "budget.spend", params: { pocket_name: "food", amount: 9000 } },
];

function Card(props: { title: string; right?: JSX.Element; children: JSX.Element }) {
  return (
    <section class={s.card}>
      <header class={s.cardHead}>
        <span class={s.cardTitle}>{props.title}</span>
        <span class={s.spacer} />
        {props.right}
      </header>
      <div class={s.cardBody}>{props.children}</div>
    </section>
  );
}

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

      {/* ★ The engine's own words. Never re-phrased — the reason names the rule
          and the numbers, and softening it would delete the useful part. */}
      <Show when={props.result.reason}>
        {(r) => <p class={s.reason}>{r()}</p>}
      </Show>

      <Show when={props.result.constraintViolated}>
        {(c) => <div class={s.reasonCode}>rule · {c()}</div>}
      </Show>

      <div class={s.reasonCode}>
        {props.result.mutations} mutation{props.result.mutations === 1 ? "" : "s"} ·{" "}
        {props.result.events.length} event{props.result.events.length === 1 ? "" : "s"}
        <Show when={props.result.verdict === "refused"}> · state unchanged</Show>
      </div>

      <Show when={props.result.events.length > 0}>
        <div style={{ "margin-top": vars.space.sm }}>
          <For each={props.result.events}>
            {(e) => <div class={s.mono}>↳ {e.name}</div>}
          </For>
        </div>
      </Show>
    </div>
  );
}

export default function App() {
  const [sustain, { refetch }] = createResource(engine.getSustain);
  const [last, setLast] = createSignal<GateResult | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [failure, setFailure] = createSignal<string | null>(null);

  const [operator, setOperator] = createSignal(PRESETS[0]!.operator);
  const [paramsText, setParamsText] = createSignal(JSON.stringify(PRESETS[0]!.params));

  const usePreset = (p: (typeof PRESETS)[number]) => {
    setOperator(p.operator);
    setParamsText(JSON.stringify(p.params));
  };

  const execute = async () => {
    setFailure(null);
    let parsed: JsonValue;
    try {
      parsed = JSON.parse(paramsText()) as JsonValue;
    } catch (e) {
      // ★ A malformed param is the UI's fault, not the gate's. Kept visibly
      //   distinct from a refusal so the two never blur together.
      setFailure(`params are not valid JSON — ${String(e)}`);
      return;
    }
    setBusy(true);
    try {
      setLast(await engine.run(operator(), parsed));
      await refetch();
    } catch (e) {
      setFailure(`the engine call itself failed — ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const reset = async () => {
    setBusy(true);
    setFailure(null);
    setLast(null);
    try {
      await engine.reset();
      await refetch();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class={s.shell}>
      <header class={s.topbar}>
        <span class={s.brand}>Mycelium</span>
        <span class={s.brandSub}>walking skeleton</span>
        <span class={s.spacer} />
        <Show when={sustain()}>
          {(su) => (
            <>
              <span
                class={s.dot}
                style={{ background: su().gateArmed ? vars.color.teal : vars.color.textDim }}
              />
              <span class={s.brandSub}>gate {su().gateArmed ? "armed" : "off"}</span>
              <span class={s.brandSub}>· {su().label}</span>
            </>
          )}
        </Show>
      </header>

      <main class={s.main}>
        {/* ── left: the Sustain ─────────────────────────────────────────── */}
        <div class={s.column}>
          <Card title="Σ · the Sustain">
            <Show
              when={sustain()}
              fallback={<div class={s.empty}>{sustain.error ? "engine unreachable" : "reading the engine…"}</div>}
            >
              {(su) => (
                <>
                  <div class={s.row}>
                    <span class={s.label}>liquid balance</span>
                    <span class={s.valueBig}>{fmt(liquidBalance(su().state))}</span>
                  </div>

                  <div style={{ "margin-top": vars.space.md }}>
                    <span class={s.label}>pockets</span>
                    <Show
                      when={pockets(su().state).length > 0}
                      fallback={<div class={s.empty}>no pockets declared</div>}
                    >
                      <For each={pockets(su().state)}>
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
                  </div>
                </>
              )}
            </Show>
          </Card>

          <Card title="V · the viable region">
            <Show
              when={sustain()}
              fallback={<div class={s.empty}>—</div>}
            >
              {(su) => (
                <For each={su().invariants} fallback={<div class={s.empty}>no invariants declared</div>}>
                  {(inv) => (
                    <div class={s.invariantRow}>
                      <span class={s.dot} style={{ background: vars.color.teal, "margin-top": "5px" }} />
                      <span>
                        <strong style={{ color: vars.color.textPrimary }}>{inv.id}</strong>
                        <br />
                        {inv.expression}
                      </span>
                    </div>
                  )}
                </For>
              )}
            </Show>
          </Card>

          <Card title="T · the moves">
            <Show when={sustain()} fallback={<div class={s.empty}>—</div>}>
              {(su) => (
                <div class={s.presetRow}>
                  <For each={su().operators}>{(op) => <span class={s.chip}>{op}</span>}</For>
                </div>
              )}
            </Show>
          </Card>
        </div>

        {/* ── right: the console ────────────────────────────────────────── */}
        <div class={s.column}>
          <Card
            title="Console · ask the gate"
            right={
              <button class={s.buttonGhost} onClick={reset} disabled={busy()}>
                reset
              </button>
            }
          >
            <div class={s.presetRow} style={{ "margin-bottom": vars.space.md }}>
              <For each={PRESETS}>
                {(p) => (
                  <button class={s.chip} onClick={() => usePreset(p)}>
                    {p.label}
                  </button>
                )}
              </For>
            </div>

            <div class={s.field}>
              <label class={s.label} for="op">operator</label>
              <input
                id="op"
                class={s.input}
                value={operator()}
                onInput={(e) => setOperator(e.currentTarget.value)}
              />
            </div>

            <div class={s.field} style={{ "margin-top": vars.space.md }}>
              <label class={s.label} for="params">params (json)</label>
              <input
                id="params"
                class={s.input}
                value={paramsText()}
                onInput={(e) => setParamsText(e.currentTarget.value)}
              />
            </div>

            <div style={{ "margin-top": vars.space.lg }}>
              <button class={s.button} onClick={execute} disabled={busy()}>
                {busy() ? "asking…" : "execute"}
              </button>
            </div>
          </Card>

          <Card title="the gate's verdict">
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
          </Card>

          <Card title="S · state, as the engine holds it">
            <Show when={sustain()} fallback={<div class={s.empty}>—</div>}>
              {(su) => <pre class={s.codeBlock}>{JSON.stringify(su().state, null, 2)}</pre>}
            </Show>
          </Card>
        </div>
      </main>
    </div>
  );
}
