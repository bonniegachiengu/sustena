/**
 * ECONOMY — the real thing, and the boundary that bounds it.
 *
 * ★★★ Everything on this screen is the engine's own machinery: `pawa::meter`
 * produced every reading, `JuulLedger` holds every entry, `Genesis::audit`
 * wrote the audit line, and `Parameters` are read out of a governance Sustain's
 * state. Nothing is computed here.
 *
 * ★★★ **Internal points only.** Juul is an accounting unit on this machine. It
 * is never real money, never transferable, never a payment rail — and the
 * notice below is carried as data from the host so a surface cannot forget it.
 * It renders through `Boundary`, which is solid rather than dashed: this is a
 * standing fact, not a gap waiting to be filled.
 */
import { createResource, createSignal, For, Show } from "solid-js";
import {
  Boundary,
  Caption,
  Card,
  Chip,
  Column,
  Empty,
  Fill,
  Label,
  Meta,
  Note,
  NoteRow,
  Readout,
  Row,
  Spacer,
  Split,
  TelemetryCell,
  TelemetryStrip,
  Value,
  Verdict,
  vars,
  sx as S,
} from "../ui";
import { engine, fmt, type GateResult } from "../lib/engine";

export default function Economy() {
  const [eco, { refetch }] = createResource(engine.economy);
  const [busy, setBusy] = createSignal(false);
  const [verdict, setVerdict] = createSignal<GateResult | null>(null);
  const [draft, setDraft] = createSignal<Record<string, string>>({});

  const change = async (name: string) => {
    const raw = draft()[name];
    if (raw === undefined || raw === "") return;
    const v = Number(raw);
    if (!Number.isFinite(v)) return;
    setBusy(true);
    try {
      // ★★★ Through the gate. There is no setter that bypasses it: an
      //   out-of-range value is refused with `enforcement_gate`, the same
      //   reason a household breach gives.
      setVerdict(await engine.setParameter(name, v));
      await refetch();
    } finally {
      setBusy(false);
    }
  };

  return (
    <Split>
      <Column>
        <Card title="juul · internal accounting" right={<Meta>{eco()?.principal ?? "—"}</Meta>}>
          <Show when={eco()} fallback={<Empty>reading the ledger…</Empty>}>
            {(e) => (
              <>
                <Readout label="balance" big>
                  {fmt(e().balance)}
                </Readout>
                <TelemetryStrip>
                  <TelemetryCell label="circulation">{fmt(e().circulation)}</TelemetryCell>
                  <TelemetryCell label="minted">{fmt(e().mintedTotal)}</TelemetryCell>
                  <TelemetryCell label="issued">{fmt(e().issuedTotal)}</TelemetryCell>
                  <TelemetryCell label="genesis">{fmt(e().genesisTotal)}</TelemetryCell>
                </TelemetryStrip>
                <NoteRow tone={e().auditClean ? "ok" : "danger"}>
                  <Value>genesis audit</Value>
                  <Caption>{e().auditDescribes}</Caption>
                  <Caption>
                    <code>circulation = Σ(mints) − Σ(costs)</code>, transfers at zero
                  </Caption>
                </NoteRow>
              </>
            )}
          </Show>
        </Card>

        {/* ★★★ The permanent boundary. Carried from the host as data. */}
        <Card title="the hard boundary">
          <Boundary title="internal points only">
            {eco()?.boundaryNotice ??
              "Internal points only. Juul is an accounting unit on this machine."}
          </Boundary>
        </Card>

        <Card title="governed parameters">
          <Show when={eco()} fallback={<Empty>reading governance…</Empty>}>
            {(e) => (
              <For each={e().parameters} fallback={<Empty>no parameters declared</Empty>}>
                {(p) => (
                  <Row>
                    <Fill>
                      <Value>{p.name}</Value>
                      <Caption>
                        in force {p.value} · genesis {p.genesis} · bounds {p.min}…{p.max}
                      </Caption>
                    </Fill>
                    <Spacer />
                    <input
                      class={S.input}
                      style={{ width: "88px" }}
                      placeholder={String(p.value)}
                      value={draft()[p.name] ?? ""}
                      onInput={(ev) => setDraft((d) => ({ ...d, [p.name]: ev.currentTarget.value }))}
                    />
                    <Chip disabled={busy()} onClick={() => void change(p.name)}>
                      set
                    </Chip>
                  </Row>
                )}
              </For>
            )}
          </Show>

          <Show when={verdict()}>
            {(v) => (
              <Note>
                <Verdict
                  verdict={v().verdict}
                  operator={v().operator}
                  reason={v().reason}
                  rule={v().constraintViolated}
                  mutations={v().mutations}
                  events={v().events.length}
                  consequence={
                    v().verdict === "refused" ? "the parameter in force is unchanged" : "in force now"
                  }
                />
              </Note>
            )}
          </Show>

          <Note>
            <Caption>
              A parameter changes only through <code>governance.set_parameter</code> — an ordinary
              Enzyme whose bounds are ordinary invariants. There is no setter that bypasses the
              gate. ★ The <em>bounds</em> are enforced here; the <em>authority</em> is not yet —
              every call in this host still runs <code>Authorization::Unchecked</code>.
            </Caption>
          </Note>
        </Card>
      </Column>

      <Column>
        <Card title="the meter · measured, not declared">
          <Show
            when={(eco()?.metered ?? []).length > 0}
            fallback={
              <Empty>nothing measured yet · run an operator and its real cost appears here</Empty>
            }
          >
            <For each={eco()!.metered}>
              {([name, m]) => (
                <div class={S.pocketBlock}>
                  <div class={S.pocketHead}>
                    <Value>{name}</Value>
                    <Meta>
                      {m.runs} run{m.runs === 1 ? "" : "s"}
                    </Meta>
                    <Value>{m.meanPawa.toFixed(2)} pawa</Value>
                  </div>
                  <Caption>
                    compute {m.totalCompute} · storage {m.totalStorage}B · total{" "}
                    {m.totalPawa.toFixed(2)}
                  </Caption>
                </div>
              )}
            </For>
          </Show>
        </Card>

        <Card title="ledger · newest first" right={<Meta>{eco()?.entries.length ?? 0} shown</Meta>}>
          <Show when={(eco()?.entries ?? []).length > 0} fallback={<Empty>no entries</Empty>}>
            <For each={eco()!.entries}>
              {(x) => (
                <div class={S.logRow}>
                  <span
                    class={S.seqCell}
                    style={{
                      color:
                        x.kind === "mint"
                          ? vars.color.teal
                          : x.kind === "debit"
                            ? vars.color.warn
                            : vars.color.info,
                    }}
                  >
                    {x.kind}
                  </span>
                  <Fill>
                    <Value>{fmt(x.amount)}</Value>{" "}
                    <Meta>
                      {x.principal}
                      {x.counterparty ? ` → ${x.counterparty}` : ""}
                    </Meta>
                    <Caption>{x.authority}</Caption>
                  </Fill>
                  <span class={S.seqCell}>
                    {x.circulationDelta > 0 ? "+" : x.circulationDelta < 0 ? "−" : "±"}
                    {fmt(Math.abs(x.circulationDelta))}
                  </span>
                </div>
              )}
            </For>
          </Show>
          <Show when={(eco()?.entries ?? []).length > 0}>
            <Note>
              <Label>every line is an entry the ledger holds, not a total this app kept</Label>
            </Note>
          </Show>
        </Card>
      </Column>
    </Split>
  );
}
