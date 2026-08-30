/**
 * COMPOSITION — `⊕`, roll-up `ρ`, and the atomic conserved transfer.
 *
 * ★★ The tree is **real and engine-checked**: `MonitorEngine::flatten_holarchy`
 * refuses a duplicate id, an unknown parent and a cycle.
 *
 * ★★★ The transfer instrument is the one this screen used to refuse to build.
 * The obvious host workaround — spend here, record income there — is two gated
 * calls, and if the second refuses the money has left one household and arrived
 * nowhere. `sustena_core::holon::transfer` decides both legs together and the
 * store commits them behind a write-ahead journal, so **both or neither** holds
 * through a crash as well as through a refusal.
 *
 * ★★ The conservation line is the ENGINE's claim, quoted. This screen does not
 * add the two balances up to check — a UI that computed the law it was
 * reporting on would be marking its own work.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import {
  Absent,
  Button,
  Caption,
  Card,
  Cluster,
  Column,
  Empty,
  ErrorState,
  Field,
  Label,
  Meta,
  Note,
  NoteRow,
  Readout,
  Rollup,
  Row,
  Spacer,
  Split,
  Sym,
  Value,
  sx as S,
} from "../ui";
import { engine, fmt, type TransferResult } from "../lib/engine";
import { openRow } from "../lib/nav";
import { childrenOf, hydrate, selectedSustain, world } from "../lib/live";

/** ★ Named in the app, not the core: the engine takes the dimension as a param. */
const LIQUID = "finances.liquid.balance";

export default function Composition() {
  const live = () => selectedSustain();
  const kids = createMemo(() => (live() ? childrenOf(live()!.summary.id) : []));
  const parent = createMemo(() => {
    const p = live()?.summary.parent;
    return p ? world.sustains[p]?.summary : undefined;
  });

  /** ★★ Only DIRECTLY linked Sustains — the engine refuses anything else, so
   *  offering a sibling here would be inviting a refusal a person could not
   *  have predicted. */
  const counterparties = createMemo(() => {
    const out = [];
    const p = parent();
    if (p) out.push(p);
    out.push(...kids());
    return out;
  });

  const [to, setTo] = createSignal<string>("");
  const [amount, setAmount] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [result, setResult] = createSignal<TransferResult | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);

  const target = () => to() || counterparties()[0]?.id || "";

  const send = async () => {
    const from = live()?.summary.id;
    const dest = target();
    const n = Number(amount());
    if (!from || !dest) return;
    if (!Number.isFinite(n)) {
      setFailure("the amount is not a number");
      return;
    }
    setFailure(null);
    setBusy(true);
    try {
      const r = await engine.transfer(from, dest, LIQUID, n);
      setResult(r);
      // ★ Both sides moved, so both are re-read. The push channel already
      //   carried the new state; this refreshes the LOG, which a push does not.
      if (r.kind === "committed") {
        await Promise.all([hydrate(from), hydrate(dest)]);
      }
    } catch (e) {
      setFailure(`the engine call failed: ${String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Split>
      <Column>
        <Card title="the tree" right={<Meta>{live()?.summary.label ?? "—"}</Meta>}>
          <NoteRow tone={world.holds ? "ok" : "danger"}>
            <Show when={world.holds} fallback={<Caption>{world.holarchyReason}</Caption>}>
              <Value>tree is valid</Value> <Meta>· {world.linked} linked</Meta>
              <Caption>
                checked by the engine: no duplicate id, no unknown parent, no cycle
              </Caption>
            </Show>
          </NoteRow>

          <Note gap="lg">
            <Label>parent</Label>
          </Note>
          <Show when={parent()} fallback={<Empty>none · this is a root Sustain</Empty>}>
            {(p) => (
              /* ★★ The parent is a door in the other direction, and was the
                 same dead end. Going UP is how somebody gets out of a habitat
                 they drilled into — without it the only way back is the
                 constellation, which is a maze wall wearing a breadcrumb. */
              <Row onClick={openRow(p().id)}>
                <Value>{p().label}</Value>
                <Meta>{p().template}</Meta>
                <Spacer />
                <Meta>{p().events} events</Meta>
                <Value>{fmt(p().liquid)}</Value>
              </Row>
            )}
          </Show>

          <Note gap="lg">
            <Label>children · {kids().length}</Label>
          </Note>
          <Show when={kids().length > 0} fallback={<Empty>no children linked</Empty>}>
            <For each={kids()}>
              {(k) => (
                /* ★★★ A linked child is the plainest "there is more here" row
                   in the cockpit: it names another Sustain that exists, is
                   loaded, and has its own everything. It was not clickable.
                   `openRow` returns a handler only when the store really holds
                   it, so a stale link renders as a fact rather than a door. */
                <Row onClick={openRow(k.id)}>
                  <Value>{k.label}</Value>
                  <Meta>{k.template}</Meta>
                  <Spacer />
                  <Meta>{k.events} events</Meta>
                  <Value>{fmt(k.liquid)}</Value>
                </Row>
              )}
            </For>
          </Show>
        </Card>

        <Card title="roll-up">
          {/* ★★★ Real, and the ENGINE's arithmetic. `sustena_core::rollup`
              folds this Sustain's own state and every linked child's into the
              declared aggregates — the host reads states and reports what came
              back, exactly as it asks `predicate::check` about a rule rather
              than judging one. Computed fresh every read, never persisted. */}
          <Rollup rollup={live()?.rollup ?? null} big />
          <Note>
            <Caption>
              Added up from this Sustain and every linked child, fresh on every read. Anything
              unreadable is named above and left out of the total.
            </Caption>
          </Note>
        </Card>
      </Column>

      <Column>
        <Card
          title="transfer between Sustains"
          right={<Meta>{live()?.summary.label ?? "—"} →</Meta>}
        >
          <Show
            when={counterparties().length > 0}
            fallback={
              <Absent title="nowhere to send">
                A transfer moves money between a parent and its child. This Sustain has
                neither, so there is nowhere to send. Link it under a household first.
              </Absent>
            }
          >
            <Field label="to">
              <select
                class={S.select}
                value={target()}
                onChange={(e) => setTo(e.currentTarget.value)}
              >
                <For each={counterparties()}>
                  {(c) => <option value={c.id}>{c.label}</option>}
                </For>
              </select>
            </Field>
            <Note>
              <Field label={`amount · moves along ${LIQUID}`}>
                <input
                  class={S.input}
                  inputmode="decimal"
                  placeholder="0"
                  value={amount()}
                  onInput={(e) => setAmount(e.currentTarget.value)}
                />
              </Field>
            </Note>
            <Note gap="lg">
              <Cluster>
                <Button onClick={send} disabled={busy() || amount().trim() === ""}>
                  {busy() ? "transferring…" : "transfer"}
                </Button>
              </Cluster>
            </Note>

            <Note>
              <Caption>
                Both sides are written together. If either is refused, both households stay as
                they were. Whole amounts only.
              </Caption>
            </Note>

            <Show when={failure()}>
              {(f) => (
                <Note>
                  <ErrorState>{f()}</ErrorState>
                </Note>
              )}
            </Show>

            <Show when={result()}>
              {(r) => (
                <Note gap="lg">
                  <Show
                    when={r().kind === "committed" ? r() : null}
                    fallback={
                      <div class={S.verdictBox.refused}>
                        <Cluster>
                          <span class={S.verdictWord.refused}>REFUSED</span>
                          <Meta>holon.transfer</Meta>
                        </Cluster>
                        <p class={S.reason}>
                          {(r() as { reason?: string }).reason ?? ""}
                        </p>
                        <div class={S.reasonCode}>
                          rule · {(r() as { rule?: string }).rule ?? "—"}
                        </div>
                        {/* ★★ The line that matters on a refusal. */}
                        <div class={S.reasonCode}>
                          nothing moved · neither side was written
                        </div>
                      </div>
                    }
                  >
                    {(c) => {
                      const t = () => c() as Extract<TransferResult, { kind: "committed" }>;
                      return (
                        <div class={S.verdictBox.admitted}>
                          <Cluster>
                            <span class={S.verdictWord.admitted}>TRANSFERRED</span>
                            <Meta>holon.transfer</Meta>
                          </Cluster>
                          <Readout label={`from · ${t().from.sustainId}`}>
                            {fmt(t().from.balanceBefore)} → {fmt(t().from.balanceAfter)}
                          </Readout>
                          <Readout label={`to · ${t().to.sustainId}`}>
                            {fmt(t().to.balanceBefore)} → {fmt(t().to.balanceAfter)}
                          </Readout>
                          {/* ★★★ Conservation, as the engine reported it. */}
                          <Readout
                            label="total across both"
                            tone={t().totalBefore === t().totalAfter ? "ok" : "danger"}
                          >
                            {fmt(t().totalBefore)} = {fmt(t().totalAfter)}
                          </Readout>
                          <div class={S.reasonCode}>
                            {t().totalBefore === t().totalAfter
                              ? "the total is unchanged, checked by the engine"
                              : "TOTAL CHANGED. The engine should not have committed this."}
                          </div>
                          <div class={S.reasonCode}>
                            both sides logged · both Sustains updated · roll-up recomputed
                          </div>
                        </div>
                      );
                    }}
                  </Show>
                </Note>
              )}
            </Show>
          </Show>
        </Card>

        <Card title="how transfers settle">
          <Caption>
            One transfer, both sides at once. Spending from one Sustain and recording income on the
            other would be two separate calls, and if the second one failed the money would leave
            one household and arrive nowhere.
          </Caption>
          <Note>
            <Caption>
              If the app closes mid-transfer, the next open finishes it and says so.
            </Caption>
          </Note>
        </Card>
      </Column>
    </Split>
  );
}
