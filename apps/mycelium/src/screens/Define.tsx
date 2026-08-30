/**
 * DEFINE — author a `Σ`, and let the engine decide whether it lands.
 *
 * ★★★ The safety machinery is the engine's: `editing::typecheck` parses every
 * invariant and **binds it against the schema**, and `editing::safe` checks the
 * candidate against every live instance. A definition that fails either is
 * **never persisted** — so the store cannot hold a `Σ` that was broken when it
 * was written.
 *
 * ★ The verdict shows the engine's own words, through the same `Verdict` the
 * Console uses. A refusal that cannot say which rule and which instance is an
 * alarm, not a diagnosis.
 */
import { createResource, createSignal, For, Show } from "solid-js";

import { onPulse } from "../lib/pulse";
import {
  Button,
  Caption,
  Card,
  Chip,
  Cluster,
  Column,
  Empty,
  ErrorState,
  Field,
  Fill,
  Label,
  Meta,
  Note,
  Row,
  Spacer,
  Split,
  Value,
  Verdict,
  vars,
  sx as S,
} from "../ui";
import { engine, type DefinitionVerdict, type DimDecl, type InvariantDecl } from "../lib/engine";
import { refreshWorld, world } from "../lib/live";

const KINDS = ["number", "text", "bool", "any"];

export default function Define() {
  const [defs, { refetch }] = createResource(engine.definitions);

  // ★★★ §III: relayed change, refreshed surface. A definition can arrive from a peer as readily as it can be authored here.
  onPulse(refetch);

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
      setFailure(`opening state is not valid JSON: ${String(e)}`);
      return;
    }
    setBusy(true);
    try {
      setVerdict(
        await engine.authorDefinition({
          id: id(),
          label: label(),
          dimensions: dims(),
          operators: ops()
            .split(",")
            .map((x) => x.trim())
            .filter(Boolean),
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
      await engine.createFromDefinition(
        `${definitionId}-${n}`,
        `${definitionId} ${n}`,
        definitionId,
        null,
      );
      await refreshWorld();
    } catch (e) {
      setFailure(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Split>
      <Column>
        <Card title="author a Sustain">
          <Row>
            <input class={S.input} value={id()} onInput={(e) => setId(e.currentTarget.value)} />
            <input
              class={S.input}
              value={label()}
              onInput={(e) => setLabel(e.currentTarget.value)}
            />
            <Meta>id · label</Meta>
          </Row>

          <Note gap="lg">
            <Label>dim(S) · declared dimensions</Label>
          </Note>
          <For each={dims()}>
            {(d, i) => (
              <Row>
                <input
                  class={S.input}
                  value={d.path}
                  onInput={(e) =>
                    setDims((v) =>
                      v.map((x, j) => (j === i() ? { ...x, path: e.currentTarget.value } : x)),
                    )
                  }
                />
                <select
                  class={S.select}
                  style={{ width: "auto" }}
                  value={d.kind}
                  onChange={(e) =>
                    setDims((v) =>
                      v.map((x, j) => (j === i() ? { ...x, kind: e.currentTarget.value } : x)),
                    )
                  }
                >
                  <For each={KINDS}>{(k) => <option value={k}>{k}</option>}</For>
                </select>
                <Spacer />
                <Chip onClick={() => setDims((v) => v.filter((_, j) => j !== i()))}>−</Chip>
              </Row>
            )}
          </For>
          <Cluster>
            <Chip
              onClick={() =>
                setDims((v) => [...v, { path: "", kind: "number", lo: null, hi: null }])
              }
            >
              + dimension
            </Chip>
          </Cluster>

          <Note gap="lg">
            <Label>rules</Label>
          </Note>
          <For each={invs()}>
            {(inv, i) => (
              <Row>
                <input
                  class={S.input}
                  value={inv.id}
                  onInput={(e) =>
                    setInvs((v) =>
                      v.map((x, j) => (j === i() ? { ...x, id: e.currentTarget.value } : x)),
                    )
                  }
                />
                <input
                  class={S.input}
                  value={inv.expression}
                  onInput={(e) =>
                    setInvs((v) =>
                      v.map((x, j) => (j === i() ? { ...x, expression: e.currentTarget.value } : x)),
                    )
                  }
                />
                <Spacer />
                <Chip onClick={() => setInvs((v) => v.filter((_, j) => j !== i()))}>−</Chip>
              </Row>
            )}
          </For>
          <Cluster>
            <Chip onClick={() => setInvs((v) => [...v, { id: "", expression: "" }])}>
              + invariant
            </Chip>
          </Cluster>

          <Note gap="lg">
            <Field label="operators (comma separated)">
              <input class={S.input} value={ops()} onInput={(e) => setOps(e.currentTarget.value)} />
            </Field>
          </Note>
          <Note>
            <Field label="opening state (json)">
              <input
                class={S.input}
                value={opening()}
                onInput={(e) => setOpening(e.currentTarget.value)}
              />
            </Field>
          </Note>

          <Note gap="lg">
            <Button onClick={author} disabled={busy()}>
              {busy() ? "checking…" : "author · the engine decides"}
            </Button>
          </Note>

          <Show when={failure()}>
            {(f) => (
              <Note>
                <ErrorState>{f()}</ErrorState>
              </Note>
            )}
          </Show>
        </Card>
      </Column>

      <Column>
        <Card title="the engine's verdict">
          <Show when={verdict()} fallback={<Empty>nothing authored yet</Empty>}>
            {(v) => (
              <Show
                when={v().kind !== "accepted"}
                fallback={
                  <Verdict
                    verdict="admitted"
                    operator="check"
                    reason="every rule checked against the schema, and no existing Sustain breaks. Saved."
                    mutations={0}
                    events={0}
                    consequence="written to the definition store"
                  />
                }
              >
                <>
                  <Verdict
                    verdict="refused"
                    operator="check"
                    rule={v().kind === "notWellTyped" ? "typecheck" : "safe to edit"}
                    reason={
                      v().kind === "notWellTyped"
                        ? "The engine refused it. Nothing was written."
                        : "Live Sustains would be pushed outside their own viable region. Nothing was written."
                    }
                    mutations={0}
                    events={0}
                    consequence="nothing persisted"
                  />
                  <For
                    each={
                      v().kind === "notWellTyped"
                        ? (v() as { errors: string[] }).errors
                        : (v() as { instances: string[] }).instances
                    }
                  >
                    {(line) => <div class={S.reasonCode}>{line}</div>}
                  </For>
                </>
              </Show>
            )}
          </Show>

          <Note>
            <Caption>
              A definition that fails either check is <strong>never persisted</strong>, so loading
              one can never surface a rule that was broken when it was authored.
            </Caption>
          </Note>
        </Card>

        <Card title="authored definitions" right={<Meta>{defs()?.length ?? 0}</Meta>}>
          <Show
            when={(defs() ?? []).length > 0}
            fallback={<Empty>none yet · the store holds only built-ins</Empty>}
          >
            <For each={defs()}>
              {(d) => (
                <Row>
                  <Fill>
                    <Value>{d.label}</Value>
                    <Caption>
                      {d.id} · {d.dimensions.length} dim · {d.invariants.length} rule
                      {d.invariants.length === 1 ? "" : "s"}
                    </Caption>
                  </Fill>
                  <Spacer />
                  <Chip disabled={busy()} onClick={() => void instantiate(d.id)}>
                    instantiate
                  </Chip>
                </Row>
              )}
            </For>
          </Show>
          <Note>
            <Caption>
              An authored definition is instantiated, gated and logged through{" "}
              <strong>exactly</strong> the path a built-in template uses. Nothing about being
              user-written makes it a second-class Sustain.
            </Caption>
          </Note>
        </Card>
      </Column>
    </Split>
  );
}
