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
 * as activity; the verdict for *this* request is held here — and it renders
 * through the SHARED `Verdict`, so a refusal reads identically wherever the
 * cockpit shows one.
 */
import { createEffect, createResource, createSignal, For, Show } from "solid-js";

import { onPulse } from "../lib/pulse";
import {
  Badge,
  Button,
  Caption,
  Card,
  Code,
  Column,
  Empty,
  ErrorState,
  Field,
  Fill,
  Label,
  Meta,
  Note,
  Readout,
  Row,
  Split,
  Value,
  Verdict,
  vars,
  sx as S,
} from "../ui";
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

export default function Console() {
  const [ops, { refetch: refetchOps }] = createResource(
    () => world.selected,
    (id) => engine.operators(id),
  );

  // ★ §III: the operator set is not fixed — installing a package adds to it,
  //   and a package can arrive from a peer. Refractory-limited like the rest.
  onPulse(refetchOps);

  const [chosen, setChosen] = createSignal<string | null>(null);
  const [paramsText, setParamsText] = createSignal("{}");
  /** Raw JSON is off by default. It is a developer view, not the answer. */
  const [raw, setRaw] = createSignal(false);
  /** One plain line about the state, so the card says something without the dump. */
  const stateSummary = () => {
    const st = live()?.state;
    if (!st || typeof st !== "object") return "nothing to show";
    const top = Object.keys(st as Record<string, unknown>);
    if (top.length === 0) return "empty";
    return `${top.length} top-level ${top.length === 1 ? "field" : "fields"}: ${top.join(", ")}`;
  };
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
      setFailure(`params are not valid JSON: ${String(e)}`);
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
      setFailure(`the engine call failed: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Split>
      <Column>
        <Card title="operators" right={<Meta>{ops()?.length ?? 0}</Meta>}>
          <Show when={(ops() ?? []).length > 0} fallback={<Empty>no operators declared</Empty>}>
            <For each={ops()}>
              {(op) => (
                <button
                  class={chosen() === op.name ? S.opRowActive : S.opRow}
                  onClick={() => pick(op)}
                >
                  <Fill>
                    <Value>{op.name}</Value>
                    <Caption>{op.description}</Caption>
                    <Caption>
                      {op.params
                        .map((p) => `${p.name}${p.required ? "" : "?"}:${p.kind}`)
                        .join(" · ") || "no params"}
                    </Caption>
                  </Fill>
                  {/* ★★★ Measured, or honestly not. */}
                  <Show
                    when={op.measured}
                    fallback={<span class={S.pawaUnmeasured}>not measured</span>}
                  >
                    {(m) => (
                      <span class={S.pawaMeasured}>
                        ~{m().meanPawa.toFixed(2)} pwa
                        <br />
                        <span class={S.caption}>
                          {m().runs} run{m().runs === 1 ? "" : "s"}
                        </span>
                      </span>
                    )}
                  </Show>
                </button>
              )}
            </For>
          </Show>
          <Note>
            <Caption>
              "not measured" means this operator has not run yet. Cost appears here once it has.
            </Caption>
          </Note>
        </Card>

        <Card title="the selected Sustain">
          <Show when={live()} fallback={<Empty>nothing selected</Empty>}>
            {(l) => (
              <>
                <Readout label="liquid balance" big>
                  {fmt(liquidBalance(l().state))}
                </Readout>
                <For each={pockets(l().state)} fallback={<Empty>no pockets yet</Empty>}>
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
        </Card>
      </Column>

      <Column>
        <Card
          title="console"
          right={<Meta>targets {live()?.summary.label ?? "nothing"}</Meta>}
        >
          <Show when={chosen()} fallback={<Empty>pick an operator on the left</Empty>}>
            {(op) => (
              <>
                <Field label="operator">
                  <input class={S.input} value={op()} readOnly />
                </Field>
                <Note>
                  <Field label="params (json)">
                    <input
                      class={S.input}
                      value={paramsText()}
                      onInput={(e) => setParamsText(e.currentTarget.value)}
                    />
                  </Field>
                </Note>
                <Note gap="lg">
                  <Button onClick={execute} disabled={busy()}>
                    {busy() ? "asking…" : "execute"}
                  </Button>
                </Note>
              </>
            )}
          </Show>
        </Card>

        <Card title="result" right={<Badge tone="ok">{world.pushes} pushed</Badge>}>
          <Show when={last()} fallback={<Empty>nothing asked yet · the gate is quiet</Empty>}>
            {(r) => (
              <Verdict
                verdict={r().verdict}
                operator={r().operator}
                reason={r().reason}
                rule={r().constraintViolated}
                mutations={r().mutations}
                events={r().events.length}
                consequence={
                  r().verdict === "refused"
                    ? "state unchanged · nothing logged · nothing charged"
                    : r().verdict === "admitted"
                      ? "logged · metered · pushed to every live screen"
                      : undefined
                }
              />
            )}
          </Show>
          <Show when={failure()}>
            {(f) => (
              <Note>
                <ErrorState>{f()}</ErrorState>
              </Note>
            )}
          </Show>
        </Card>

        <Card
          title="state"
          right={
            <Show when={live()?.state}>
              <button class={S.chip} onClick={() => setRaw((r) => !r)}>
                {raw() ? "hide raw" : "raw"}
              </button>
            </Show>
          }
        >
          <Show when={live()?.state} fallback={<Empty>nothing selected</Empty>}>
            <Show when={raw()} fallback={<Caption>{stateSummary()}</Caption>}>
              <Code>{JSON.stringify(live()!.state, null, 2)}</Code>
            </Show>
          </Show>
        </Card>
      </Column>
    </Split>
  );
}
