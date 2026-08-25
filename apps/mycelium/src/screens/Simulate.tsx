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
 * branch is a value — and every hypothetical on this screen renders through the
 * shared `Hypothetical`, which is amber and dashed so it can never be misread
 * as something that happened.
 */
import { createSignal, For, Show } from "solid-js";
import {
  Absent,
  Badge,
  Button,
  Card,
  Chip,
  Cluster,
  Column,
  Empty,
  ErrorState,
  Field,
  Hypothetical,
  Label,
  Meta,
  Note,
  Readout,
  Row,
  Split,
  Sym,
  Value,
  Verdict,
  vars,
  sx as S,
} from "../ui";
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
      setFailure(`params are not valid JSON: ${String(e)}`);
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
            `stopped at ${st.operator} after ${done.length} step(s). Refused: ${r.reason ?? r.constraintViolated ?? "no reason given"}`,
          );
          return;
        }
        done.push(st.operator);
      }
      setPromoted(`promoted. ${done.length} step(s) committed for real.`);
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
    <Split>
      <Column>
        <Card title="branch" right={<Meta>{live()?.summary.label ?? "—"}</Meta>}>
          <Hypothetical title="◆ hypothetical">
            A branch runs against a copy of live state, through the same checks a real call
            uses. Nothing is written. Promoting runs it for real, and can still be refused if
            live state has changed since.
          </Hypothetical>

          <Note>
            <For each={steps()}>
              {(st, i) => (
                <div style={{ "margin-bottom": vars.space.md }}>
                  <Field label={`step ${i() + 1}`}>
                    <input
                      class={S.input}
                      value={st.operator}
                      onInput={(e) =>
                        setSteps((v) =>
                          v.map((x, j) =>
                            j === i() ? { ...x, operator: e.currentTarget.value } : x,
                          ),
                        )
                      }
                    />
                    <input
                      class={S.input}
                      value={st.params}
                      onInput={(e) =>
                        setSteps((v) =>
                          v.map((x, j) => (j === i() ? { ...x, params: e.currentTarget.value } : x)),
                        )
                      }
                    />
                  </Field>
                </div>
              )}
            </For>
          </Note>

          <Cluster>
            <Chip
              onClick={() =>
                setSteps((v) => [
                  ...v,
                  { operator: "budget.allocate", params: '{"pocket_name":"food","amount":500}' },
                ])
              }
            >
              + step
            </Chip>
            <Show when={steps().length > 1}>
              <Chip onClick={() => setSteps((v) => v.slice(0, -1))}>− step</Chip>
            </Show>
          </Cluster>

          <Note gap="lg">
            <Cluster>
              <Button onClick={run} disabled={busy() || !world.selected}>
                {busy() ? "forking…" : "run branch"}
              </Button>
              <Show when={branch()}>
                <Button variant="ghost" onClick={promote} disabled={busy()}>
                  promote to reality
                </Button>
              </Show>
            </Cluster>
          </Note>

          <Show when={failure()}>
            {(f) => (
              <Note>
                <ErrorState>{f()}</ErrorState>
              </Note>
            )}
          </Show>
          <Show when={promoted()}>
            {(p) => (
              <Note>
                <Verdict
                  verdict="admitted"
                  operator="promote"
                  reason={p()}
                  mutations={0}
                  events={0}
                  consequence="every step was run again against live state"
                />
              </Note>
            )}
          </Show>
        </Card>

        <Card title="roll-up">
          <Absent title="not available on a branch">
            A branch does not compute a household total.
          </Absent>
        </Card>
      </Column>

      <Column>
        <Card
          title="the branch"
          right={
            <Show when={branch()}>
              <Badge tone="warn">◆ nothing written</Badge>
            </Show>
          }
        >
          <Show
            when={branch()}
            fallback={<Empty>no branch yet · run one to see what would happen</Empty>}
          >
            {(b) => (
              <>
                <For each={b().steps}>
                  {(st, i) => (
                    <div style={{ "margin-bottom": vars.space.sm }}>
                      <Verdict
                        verdict={st.verdict}
                        operator={`${i() + 1}. ${st.operator}`}
                        reason={st.reason}
                        mutations={st.mutations}
                        events={st.events.length}
                        consequence={
                          st.verdict === "refused" ? "the branch stops here" : "in the fork only"
                        }
                      />
                    </div>
                  )}
                </For>

                <Show when={finalState()}>
                  {(fin) => (
                    <>
                      <Note gap="lg">
                        <Label>state diff · live → hypothetical</Label>
                      </Note>
                      <For each={diff(b().from, fin())} fallback={<Empty>nothing moved</Empty>}>
                        {(d) => (
                          <div class={S.diffRow}>
                            <span class={S.meta}>{d.path}</span>
                            <span class={S.seqCell}>{d.from}</span>
                            <Value>→ {d.to}</Value>
                          </div>
                        )}
                      </For>

                      <Note>
                        <Readout label="liquid · hypothetical" big tone="warn">
                          {fmt(liquidBalance(fin()))}
                        </Readout>
                      </Note>
                      <For each={pockets(fin())}>
                        {(p) => (
                          <Row>
                            <Value>{p.name}</Value>
                            <Meta>alloc {fmt(p.allocated)}</Meta>
                            <Meta>spent {fmt(p.spent)}</Meta>
                            <Value>left {fmt(p.left)}</Value>
                          </Row>
                        )}
                      </For>
                    </>
                  )}
                </Show>
              </>
            )}
          </Show>
        </Card>
      </Column>
    </Split>
  );
}
