/**
 * DEFINE — author a `Σ`, and let the engine decide whether it lands.
 *
 * ★★★ The safety machinery is the engine's: `editing::typecheck` parses every
 * invariant and **binds it against the schema**, and `editing::safe` checks the
 * candidate against every live instance. A definition that fails either is
 * **never persisted** — so the store cannot hold a `Σ` that was broken when it
 * was written.
 *
 * ★ The verdict shows the engine's own words. A refusal that cannot say which
 * rule and which instance is an alarm, not a diagnosis.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import * as s from "../styles/app.css";
import { vars } from "../styles/tokens.css";
import { engine, type DefinitionVerdict, type DimDecl, type InvariantDecl } from "../lib/engine";
import { refreshWorld, world } from "../lib/live";

const KINDS = ["number", "text", "bool", "any"];

export default function Define() {
  const [defs, { refetch }] = createResource(engine.definitions);

  const [id, setId] = createSignal("garden");
  const [label, setLabel] = createSignal("Garden");
  const [dims, setDims] = createSignal<DimDecl[]>([
    { path: "soil", kind: "number", lo: 0, hi: 100 },
  ]);
  const [invs, setInvs] = createSignal<InvariantDecl[]>([
    { id: "soil_ok", expression: "soil >= 0" },
  ]);
  const [ops, setOps] = createSignal("budget.record_income");
  const [opening, setOpening] = createSignal('{"soil":40}');
  const [verdict, setVerdict] = createSignal<DefinitionVerdict | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [failure, setFailure] = createSignal<string | null>(null);

  const author = async () => {
    setFailure(null);
    let openingState: unknown;
    try {
      openingState = JSON.parse(opening());
    } catch (e) {
      setFailure(`opening state is not valid JSON — ${String(e)}`);
      return;
    }
    setBusy(true);
    try {
      setVerdict(
        await engine.authorDefinition({
          id: id(),
          label: label(),
          dimensions: dims(),
          operators: ops().split(",").map((x) => x.trim()).filter(Boolean),
          invariants: invs(),
          openingState: openingState as never,
        }),
      );
      await refetch();
    } catch (e) {
      setFailure(String(e));
    } finally {
      setBusy(false);
    }
  };

  const instantiate = async (definitionId: string) => {
    setBusy(true);
    try {
      const n = world.order.filter((k) => k.startsWith(definitionId)).length + 1;
      await engine.createFromDefinition(`${definitionId}-${n}`, `${definitionId} ${n}`, definitionId, null);
      await refreshWorld();
    } catch (e) {
      setFailure(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class={s.main}>
      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>author Σ</span>
          </header>
          <div class={s.cardBody}>
            <div class={s.paramRow}>
              <input class={s.input} value={id()} onInput={(e) => setId(e.currentTarget.value)} />
              <input class={s.input} value={label()} onInput={(e) => setLabel(e.currentTarget.value)} />
              <span class={s.attentionWhy}>id · label</span>
            </div>

            <div class={s.label} style={{ "margin-top": vars.space.lg }}>dim(S) · declared dimensions</div>
            <For each={dims()}>
              {(d, i) => (
                <div class={s.paramRow}>
                  <input
                    class={s.input}
                    value={d.path}
                    onInput={(e) =>
                      setDims((v) => v.map((x, j) => (j === i() ? { ...x, path: e.currentTarget.value } : x)))
                    }
                  />
                  <select
                    class={s.picker}
                    value={d.kind}
                    onChange={(e) =>
                      setDims((v) => v.map((x, j) => (j === i() ? { ...x, kind: e.currentTarget.value } : x)))
                    }
                  >
                    <For each={KINDS}>{(k) => <option value={k}>{k}</option>}</For>
                  </select>
                  <button
                    class={s.chip}
                    onClick={() => setDims((v) => v.filter((_, j) => j !== i()))}
                  >
                    −
                  </button>
                </div>
              )}
            </For>
            <button
              class={s.chip}
              onClick={() => setDims((v) => [...v, { path: "", kind: "number", lo: null, hi: null }])}
            >
              + dimension
            </button>

            <div class={s.label} style={{ "margin-top": vars.space.lg }}>V · invariants</div>
            <For each={invs()}>
              {(inv, i) => (
                <div class={s.paramRow}>
                  <input
                    class={s.input}
                    value={inv.id}
                    onInput={(e) =>
                      setInvs((v) => v.map((x, j) => (j === i() ? { ...x, id: e.currentTarget.value } : x)))
                    }
                  />
                  <input
                    class={s.input}
                    value={inv.expression}
                    onInput={(e) =>
                      setInvs((v) =>
                        v.map((x, j) => (j === i() ? { ...x, expression: e.currentTarget.value } : x)),
                      )
                    }
                  />
                  <button class={s.chip} onClick={() => setInvs((v) => v.filter((_, j) => j !== i()))}>
                    −
                  </button>
                </div>
              )}
            </For>
            <button
              class={s.chip}
              onClick={() => setInvs((v) => [...v, { id: "", expression: "" }])}
            >
              + invariant
            </button>

            <div class={s.field} style={{ "margin-top": vars.space.lg }}>
              <label class={s.label}>T · operators (comma separated)</label>
              <input class={s.input} value={ops()} onInput={(e) => setOps(e.currentTarget.value)} />
            </div>
            <div class={s.field} style={{ "margin-top": vars.space.md }}>
              <label class={s.label}>opening state (json)</label>
              <input class={s.input} value={opening()} onInput={(e) => setOpening(e.currentTarget.value)} />
            </div>

            <div style={{ "margin-top": vars.space.lg }}>
              <button class={s.button} onClick={author} disabled={busy()}>
                {busy() ? "checking…" : "author · the engine decides"}
              </button>
            </div>

            <Show when={failure()}>
              {(f) => <div class={s.errorBox} style={{ "margin-top": vars.space.md }}>{f()}</div>}
            </Show>
          </div>
        </section>
      </div>

      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>the engine's verdict</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={verdict()}
              fallback={<div class={s.empty}>nothing authored yet</div>}
            >
              {(v) => (
                <Show
                  when={v().kind !== "accepted"}
                  fallback={
                    <div class={s.verdictAdmitted}>
                      <span class={s.badgeAdmitted}>ACCEPTED</span>
                      <p class={s.reason}>
                        `typecheck` bound every invariant against the schema and no live instance
                        would be stranded. Persisted.
                      </p>
                    </div>
                  }
                >
                  <div class={s.verdictRefused}>
                    <span class={s.badgeRefused}>
                      {v().kind === "notWellTyped" ? "NOT WELL-TYPED" : "WOULD STRAND"}
                    </span>
                    <p class={s.reason}>
                      {v().kind === "notWellTyped"
                        ? "The engine refused it. Nothing was written."
                        : "Live Sustains would be pushed outside their own viable region. Nothing was written."}
                    </p>
                    <For
                      each={
                        v().kind === "notWellTyped"
                          ? (v() as { errors: string[] }).errors
                          : (v() as { instances: string[] }).instances
                      }
                    >
                      {(line) => <div class={s.reasonCode}>{line}</div>}
                    </For>
                  </div>
                </Show>
              )}
            </Show>

            <div class={s.attentionWhy} style={{ "margin-top": vars.space.md }}>
              A definition that fails either check is <strong>never persisted</strong>, so loading
              one can never surface a rule that was broken when it was authored.
            </div>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>authored definitions</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>{defs()?.length ?? 0}</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={(defs() ?? []).length > 0}
              fallback={<div class={s.empty}>none yet · the store holds only built-ins</div>}
            >
              <For each={defs()}>
                {(d) => (
                  <div class={s.paramRow}>
                    <span>
                      <span class={s.value}>{d.label}</span>
                      <br />
                      <span class={s.attentionWhy}>
                        {d.id} · {d.dimensions.length} dim · {d.invariants.length} rule
                        {d.invariants.length === 1 ? "" : "s"}
                      </span>
                    </span>
                    <span />
                    <button class={s.chip} disabled={busy()} onClick={() => void instantiate(d.id)}>
                      instantiate
                    </button>
                  </div>
                )}
              </For>
            </Show>
            <div class={s.attentionWhy} style={{ "margin-top": vars.space.md }}>
              An authored definition is instantiated, gated and logged through{" "}
              <strong>exactly</strong> the path a built-in template uses. Nothing about being
              user-written makes it a second-class Sustain.
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
