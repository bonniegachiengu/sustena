/**
 * MYCELIUM — V1.1.
 *
 * A real, persistent, multi-Sustain cockpit. The household lives in an
 * append-only event log on disk; every state on screen was folded from it.
 *
 * ★ Still one screen. The Monitor, Constellation, Simulate, Define, Economy and
 * Council surfaces are later slices — and where the engine cannot yet do
 * something (roll-up `ρ`), this renders the honest unavailable state and says
 * why, rather than computing it in the host and passing host arithmetic off as
 * an engine capability.
 */
import { createResource, createSignal, For, Show, type JSX } from "solid-js";
import * as s from "./styles/app.css";
import { vars } from "./styles/tokens.css";
import {
  childrenOf,
  engine,
  fmt,
  liquidBalance,
  pockets,
  type GateResult,
  type JsonValue,
  type SustainSummary,
} from "./lib/engine";

const PRESETS: { label: string; operator: string; params: Record<string, unknown> }[] = [
  { label: "income 4500", operator: "budget.record_income", params: { amount: 4500, source: "salary" } },
  { label: "add pocket", operator: "budget.add_pocket", params: { pocket_name: "food" } },
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
      <Show when={props.result.reason}>{(r) => <p class={s.reason}>{r()}</p>}</Show>
      <Show when={props.result.constraintViolated}>
        {(c) => <div class={s.reasonCode}>rule · {c()}</div>}
      </Show>

      <div class={s.reasonCode}>
        {props.result.mutations} mutation{props.result.mutations === 1 ? "" : "s"} ·{" "}
        {props.result.events.length} event{props.result.events.length === 1 ? "" : "s"}
        <Show when={props.result.verdict === "refused"}> · state unchanged · nothing logged</Show>
        <Show when={props.result.verdict === "admitted"}> · appended to the log</Show>
      </div>

      <Show when={props.result.events.length > 0}>
        <div style={{ "margin-top": vars.space.sm }}>
          <For each={props.result.events}>{(e) => <div class={s.mono}>↳ {e.name}</div>}</For>
        </div>
      </Show>
    </div>
  );
}

export default function App() {
  const [world, { refetch: refetchWorld }] = createResource(engine.world);
  const [sustain, { refetch: refetchSustain }] = createResource(
    () => world()?.selected ?? null,
    (id) => engine.sustain(id),
  );

  const [last, setLast] = createSignal<GateResult | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [failure, setFailure] = createSignal<string | null>(null);

  const [operator, setOperator] = createSignal(PRESETS[0]!.operator);
  const [paramsText, setParamsText] = createSignal(JSON.stringify(PRESETS[0]!.params));

  const refreshAll = async () => {
    await refetchWorld();
    await refetchSustain();
  };

  const usePreset = (p: (typeof PRESETS)[number]) => {
    setOperator(p.operator);
    setParamsText(JSON.stringify(p.params));
  };

  const pick = async (id: string) => {
    setBusy(true);
    setFailure(null);
    setLast(null);
    try {
      await engine.select(id);
      await refreshAll();
    } catch (e) {
      setFailure(`could not select — ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const execute = async () => {
    setFailure(null);
    const target = world()?.selected;
    if (!target) {
      setFailure("no Sustain selected");
      return;
    }
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
      const r = await engine.run(target, operator(), parsed);
      if (r === null) {
        setFailure(`the engine has no Sustain called "${target}"`);
      } else {
        setLast(r);
      }
      await refreshAll();
    } catch (e) {
      setFailure(`the engine call itself failed — ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  const roots = () => (world()?.sustains ?? []).filter((x) => x.parent === null);

  const SustainRow = (props: { row: SustainSummary; child?: boolean }) => (
    <button
      class={`${world()?.selected === props.row.id ? s.sustainRowActive : s.sustainRow} ${
        props.child ? s.childIndent : ""
      }`}
      onClick={() => void pick(props.row.id)}
      disabled={busy()}
    >
      <span
        class={s.dot}
        style={{
          background:
            world()?.selected === props.row.id ? vars.color.amber : vars.color.borderLight,
        }}
      />
      <span>
        <span class={s.sustainName}>{props.row.label}</span>
        <br />
        <span class={s.sustainMeta}>
          {props.row.template} · {props.row.events} event{props.row.events === 1 ? "" : "s"}
        </span>
      </span>
      <span class={s.sustainFigure}>{fmt(props.row.liquid)}</span>
    </button>
  );

  return (
    <div class={s.shell}>
      <header class={s.topbar}>
        <span class={s.brand}>Mycelium</span>
        <span class={s.brandSub}>v1.1 · persistent</span>
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
        {/* ── left: the household ───────────────────────────────────────── */}
        <div class={s.column}>
          <Card
            title="the household"
            right={
              <span class={s.sustainMeta}>{world()?.sustains.length ?? 0} sustains</span>
            }
          >
            <Show
              when={world()}
              fallback={
                <div class={s.empty}>
                  {world.error ? "engine unreachable" : "opening the household…"}
                </div>
              }
            >
              {(w) => (
                <Show
                  when={w().sustains.length > 0}
                  fallback={<div class={s.empty}>no sustains yet · the store is empty</div>}
                >
                  <div class={s.selector}>
                    <For each={roots()}>
                      {(root) => (
                        <>
                          <SustainRow row={root} />
                          <For each={childrenOf(w(), root.id)}>
                            {(kid) => <SustainRow row={kid} child />}
                          </For>
                        </>
                      )}
                    </For>
                  </div>
                </Show>
              )}
            </Show>
          </Card>

          <Card title="⊕ · composition">
            <Show when={world()} fallback={<div class={s.empty}>—</div>}>
              {(w) => (
                <>
                  <div class={s.invariantRow}>
                    <span
                      class={s.dot}
                      style={{
                        background:
                          w().holarchy.kind === "holds" ? vars.color.teal : vars.color.danger,
                        "margin-top": "5px",
                      }}
                    />
                    <span>
                      {w().holarchy.kind === "holds" ? (
                        <>
                          <strong style={{ color: vars.color.textPrimary }}>tree holds</strong>
                          <br />
                          {(w().holarchy as { linked: number }).linked} linked · validated by the
                          engine's own <code>MonitorEngine::flatten_holarchy</code> — no duplicate
                          id, no unknown parent, no cycle
                        </>
                      ) : (
                        <>
                          <strong style={{ color: vars.color.danger }}>tree broken</strong>
                          <br />
                          {(w().holarchy as { reason: string }).reason}
                        </>
                      )}
                    </span>
                  </div>

                  {/* ★★★ The honest unavailable state. Roll-up ρ does not exist
                      in sustena-core; summing the children here would be host
                      arithmetic wearing an engine's name. */}
                  <Show when={!w().rollupAvailable}>
                    <div class={s.unavailable} style={{ "margin-top": vars.space.md }}>
                      <span class={s.unavailableTitle}>roll-up ρ · not available</span>
                      The engine can hold this tree and check it, but it cannot yet fold a
                      child's state into a parent aggregate. That capability exists in the
                      Python engine and has not been ported to <code>sustena-core</code>.
                      <br />
                      <br />
                      Nothing is summed here. A household total computed by this app rather
                      than by the engine would be a number with no rule behind it.
                    </div>
                  </Show>
                </>
              )}
            </Show>
          </Card>

          <Card title="V · the viable region">
            <Show when={sustain()} fallback={<div class={s.empty}>—</div>}>
              {(su) => (
                <For
                  each={su().invariants}
                  fallback={<div class={s.empty}>no invariants declared</div>}
                >
                  {(inv) => (
                    <div class={s.invariantRow}>
                      <span
                        class={s.dot}
                        style={{ background: vars.color.teal, "margin-top": "5px" }}
                      />
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
        </div>

        {/* ── right: the selected Sustain ───────────────────────────────── */}
        <div class={s.column}>
          <Card
            title="Σ · the selected Sustain"
            right={<span class={s.sustainMeta}>{sustain()?.id ?? "—"}</span>}
          >
            <Show when={sustain()} fallback={<div class={s.empty}>nothing selected</div>}>
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
                      fallback={<div class={s.empty}>no pockets yet</div>}
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
                  <div style={{ "margin-top": vars.space.md }}>
                    <span class={s.label}>T · the moves</span>
                    <div class={s.presetRow} style={{ "margin-top": vars.space.xs }}>
                      <For each={su().operators}>{(op) => <span class={s.chip}>{op}</span>}</For>
                    </div>
                  </div>
                </>
              )}
            </Show>
          </Card>

          <Card title="Console · ask the gate">
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
              <label class={s.label} for="op">
                operator · targets {sustain()?.label ?? "nothing"}
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
              <button class={s.button} onClick={execute} disabled={busy() || !world()?.selected}>
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

      {/* ── the status belt ───────────────────────────────────────────────── */}
      <footer class={s.statusBelt}>
        <span>store</span>
        <span style={{ color: vars.color.textSecondary }}>{world()?.storePath ?? "—"}</span>
        <span class={s.spacer} />
        <span>state = fold(events) · rebuilt from the log on every launch</span>
      </footer>
    </div>
  );
}
