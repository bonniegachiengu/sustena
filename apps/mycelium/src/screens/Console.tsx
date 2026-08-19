/**
 * CONSOLE — the full gate console.
 *
 * ★★ The picker lists the **selected Sustain's own `T`**, not the whole
 * registry: an operator the definition does not permit would be refused with
 * `operator_allowed`, and offering it would be inviting a refusal the person
 * could not have predicted.
 *
 * ★★★ Each one shows its **measured** pawa where the meter has a reading and
 * *not measured* where it does not. A `0` from an unmeasured basis is silence,
 * not cheapness — so the two never render the same.
 *
 * ★ The one screen where a refusal belongs. The live store carries refusals only
 * as activity; the verdict for *this* request is held here.
 */
import { createEffect, createResource, createSignal, For, Show } from "solid-js";
import * as s from "../styles/app.css";
import { vars } from "../styles/tokens.css";
import {
  engine,
  fmt,
  liquidBalance,
  pockets,
  type GateResult,
  type JsonValue,
  type OperatorDto,
} from "../lib/engine";
import { selectedSustain, world } from "../lib/live";

/** A starting `θ` from the operator's declared params — never a guess at values. */
function draftParams(op: OperatorDto, state: unknown): string {
  const out: Record<string, unknown> = {};
  for (const p of op.params) {
    if (!p.required) continue;
    if (p.namesWithin) {
      // ★★ The operator declares WHICH state path holds the legal values, so the
      //    picker can offer a real key without this app knowing what a pocket is.
      const bag = p.namesWithin
        .split(".")
        .reduce<any>((acc, k) => (acc == null ? acc : acc[k]), state as any);
      const first = bag && typeof bag === "object" ? Object.keys(bag)[0] : undefined;
      out[p.name] = first ?? "";
    } else if (p.kind === "number") {
      out[p.name] = 0;
    } else {
      out[p.name] = "";
    }
  }
  return JSON.stringify(out);
}

function Verdict(props: { result: GateResult }) {
  const v = () => props.result.verdict;
  const box = () =>
    v() === "admitted" ? s.verdictAdmitted : v() === "refused" ? s.verdictRefused : s.verdictDeferred;
  const badge = () =>
    v() === "admitted" ? s.badgeAdmitted : v() === "refused" ? s.badgeRefused : s.badgeDeferred;

  return (
    <div class={box()}>
      <div style={{ display: "flex", "align-items": "baseline", gap: vars.space.md }}>
        <span class={badge()}>{v().toUpperCase()}</span>
        <span class={s.mono}>{props.result.operator}</span>
      </div>
      <Show when={props.result.reason}>{(r) => <p class={s.reason}>{r()}</p>}</Show>
      <Show when={props.result.constraintViolated}>
        {(c) => <div class={s.reasonCode}>rule · {c()}</div>}
      </Show>
      <div class={s.reasonCode}>
        {props.result.mutations} mutation{props.result.mutations === 1 ? "" : "s"} ·{" "}
        {props.result.events.length} event{props.result.events.length === 1 ? "" : "s"}
        <Show when={v() === "refused"}> · state unchanged · nothing logged · nothing charged</Show>
        <Show when={v() === "admitted"}> · logged · metered · pushed to every live screen</Show>
      </div>
    </div>
  );
}

export default function Console() {
  const [ops, { refetch: refetchOps }] = createResource(
    () => world.selected,
    (id) => engine.operators(id),
  );

  const [chosen, setChosen] = createSignal<string | null>(null);
  const [paramsText, setParamsText] = createSignal("{}");
  const [last, setLast] = createSignal<GateResult | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [failure, setFailure] = createSignal<string | null>(null);

  const live = () => selectedSustain();

  // Reset the picker when the subject changes — a θ drafted for one Sustain's
  // pockets is not meaningful for another's.
  createEffect(() => {
    world.selected;
    setChosen(null);
    setParamsText("{}");
    setLast(null);
  });

  const pick = (op: OperatorDto) => {
    setChosen(op.name);
    setParamsText(draftParams(op, live()?.state));
    setFailure(null);
  };

  const execute = async () => {
    const target = world.selected;
    const op = chosen();
    if (!target || !op) return;
    setFailure(null);
    let parsed: JsonValue;
    try {
      parsed = JSON.parse(paramsText()) as JsonValue;
    } catch (e) {
      setFailure(`params are not valid JSON — ${String(e)}`);
      return;
    }
    setBusy(true);
    try {
      const r = await engine.run(target, op, parsed);
      if (r === null) setFailure(`the engine has no Sustain called "${target}"`);
      else setLast(r);
      // ★ No state refetch — a commit reaches every live screen through the
      //   push channel. The catalogue IS refetched, because a run changes the
      //   measured pawa and that number is the point.
      await refetchOps();
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
            <span class={s.cardTitle}>T · operators on this Sustain</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>{ops()?.length ?? 0}</span>
          </header>
          <div class={s.cardBody} style={{ padding: 0 }}>
            <Show
              when={(ops() ?? []).length > 0}
              fallback={<div class={s.empty} style={{ padding: vars.space.md }}>no operators declared</div>}
            >
              <For each={ops()}>
                {(op) => (
                  <button
                    class={chosen() === op.name ? s.opRowActive : s.opRow}
                    onClick={() => pick(op)}
                  >
                    <span>
                      <span class={s.value}>{op.name}</span>
                      <br />
                      <span class={s.attentionWhy}>{op.description}</span>
                      <br />
                      <span class={s.attentionWhy}>
                        {op.params.map((p) => `${p.name}${p.required ? "" : "?"}:${p.kind}`).join(" · ") ||
                          "no params"}
                      </span>
                    </span>
                    {/* ★★★ Measured, or honestly not. */}
                    <Show
                      when={op.measured}
                      fallback={<span class={s.pawaUnmeasured}>not measured</span>}
                    >
                      {(m) => (
                        <span class={s.pawaMeasured}>
                          ~{m().meanPawa.toFixed(2)} pwa
                          <br />
                          <span class={s.attentionWhy}>{m().runs} run{m().runs === 1 ? "" : "s"}</span>
                        </span>
                      )}
                    </Show>
                  </button>
                )}
              </For>
            </Show>
            <div class={s.attentionWhy} style={{ padding: vars.space.md }}>
              "not measured" means the meter has never seen it run — not that it is free. The
              author's declared estimate is usually <code>0</code>; the meter is the truth.
            </div>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>Σ · the selected Sustain</span>
          </header>
          <div class={s.cardBody}>
            <Show when={live()} fallback={<div class={s.empty}>nothing selected</div>}>
              {(l) => (
                <>
                  <div class={s.row}>
                    <span class={s.label}>liquid balance</span>
                    <span class={s.valueBig}>{fmt(liquidBalance(l().state))}</span>
                  </div>
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
                </>
              )}
            </Show>
          </div>
        </section>
      </div>

      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>console · ask the gate</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>targets {live()?.summary.label ?? "nothing"}</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={chosen()}
              fallback={<div class={s.empty}>pick an operator on the left</div>}
            >
              {(op) => (
                <>
                  <div class={s.field}>
                    <label class={s.label} for="op">operator</label>
                    <input id="op" class={s.input} value={op()} readOnly />
                  </div>
                  <div class={s.field} style={{ "margin-top": vars.space.md }}>
                    <label class={s.label} for="params">
                      params (json · objects and arrays supported)
                    </label>
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
                </>
              )}
            </Show>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>the gate's verdict</span>
            <span class={s.spacer} />
            <span class={s.pushBadge}>{world.pushes} pushed</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={last()}
              fallback={<div class={s.empty}>nothing asked yet · the gate is quiet</div>}
            >
              {(r) => <Verdict result={r()} />}
            </Show>
            <Show when={failure()}>
              {(f) => <div class={s.errorBox} style={{ "margin-top": vars.space.md }}>{f()}</div>}
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
    </div>
  );
}
