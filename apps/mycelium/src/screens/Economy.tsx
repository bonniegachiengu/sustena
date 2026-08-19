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
 */
import { createResource, createSignal, For, Show } from "solid-js";
import * as s from "../styles/app.css";
import { vars } from "../styles/tokens.css";
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
    <div class={s.main}>
      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>juul · internal accounting</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>{eco()?.principal ?? "—"}</span>
          </header>
          <div class={s.cardBody}>
            <Show when={eco()} fallback={<div class={s.empty}>reading the ledger…</div>}>
              {(e) => (
                <>
                  <div class={s.row}>
                    <span class={s.label}>balance</span>
                    <span class={s.valueBig}>{fmt(e().balance)}</span>
                  </div>
                  <div class={s.telemetry}>
                    <div class={s.telemetryCell}>
                      <span class={s.label}>circulation</span>
                      <span class={s.value}>{fmt(e().circulation)}</span>
                    </div>
                    <div class={s.telemetryCell}>
                      <span class={s.label}>minted</span>
                      <span class={s.value}>{fmt(e().mintedTotal)}</span>
                    </div>
                    <div class={s.telemetryCell}>
                      <span class={s.label}>issued</span>
                      <span class={s.value}>{fmt(e().issuedTotal)}</span>
                    </div>
                    <div class={s.telemetryCell}>
                      <span class={s.label}>genesis</span>
                      <span class={s.value}>{fmt(e().genesisTotal)}</span>
                    </div>
                  </div>
                  <div class={s.invariantRow}>
                    <span
                      class={s.dot}
                      style={{
                        background: e().auditClean ? vars.color.teal : vars.color.danger,
                        "margin-top": "5px",
                      }}
                    />
                    <span>
                      <strong style={{ color: vars.color.textPrimary }}>genesis audit</strong>
                      <br />
                      {e().auditDescribes}
                      <br />
                      <span class={s.attentionWhy}>
                        `circulation = Σ(mints) − Σ(costs)`, transfers at zero
                      </span>
                    </span>
                  </div>
                </>
              )}
            </Show>
          </div>
        </section>

        {/* ★★★ The permanent boundary. Carried from the host as data. */}
        <section class={s.card}>
          <div class={s.cardBody}>
            <div class={s.boundary}>
              <span class={s.unavailableTitle}>the hard boundary</span>
              {eco()?.boundaryNotice ??
                "Internal points only. Juul is an accounting unit on this machine."}
            </div>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>governed parameters</span>
          </header>
          <div class={s.cardBody}>
            <Show when={eco()} fallback={<div class={s.empty}>—</div>}>
              {(e) => (
                <For each={e().parameters}>
                  {(p) => (
                    <div class={s.paramRow}>
                      <span>
                        <span class={s.value}>{p.name}</span>
                        <br />
                        <span class={s.attentionWhy}>
                          in force {p.value} · genesis {p.genesis} · bounds {p.min}…{p.max}
                        </span>
                      </span>
                      <input
                        class={s.input}
                        style={{ width: "88px" }}
                        placeholder={String(p.value)}
                        value={draft()[p.name] ?? ""}
                        onInput={(ev) =>
                          setDraft((d) => ({ ...d, [p.name]: ev.currentTarget.value }))
                        }
                      />
                      <button class={s.chip} disabled={busy()} onClick={() => void change(p.name)}>
                        set
                      </button>
                    </div>
                  )}
                </For>
              )}
            </Show>

            <Show when={verdict()}>
              {(v) => (
                <div
                  class={v().verdict === "admitted" ? s.verdictAdmitted : s.verdictRefused}
                  style={{ "margin-top": vars.space.md }}
                >
                  <span class={v().verdict === "admitted" ? s.badgeAdmitted : s.badgeRefused}>
                    {v().verdict.toUpperCase()}
                  </span>
                  <Show when={v().reason}>{(r) => <p class={s.reason}>{r()}</p>}</Show>
                  <Show when={v().constraintViolated}>
                    {(c) => <div class={s.reasonCode}>rule · {c()}</div>}
                  </Show>
                </div>
              )}
            </Show>

            <div class={s.attentionWhy} style={{ "margin-top": vars.space.md }}>
              A parameter changes only through <code>governance.set_parameter</code> — an ordinary
              Enzyme whose bounds are ordinary invariants. There is no setter that bypasses the
              gate. ★ The <em>bounds</em> are enforced here; the <em>authority</em> is not yet —
              every call in this host still runs <code>Authorization::Unchecked</code>.
            </div>
          </div>
        </section>
      </div>

      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>the meter · measured, not declared</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={(eco()?.metered ?? []).length > 0}
              fallback={
                <div class={s.empty}>
                  nothing measured yet · run an operator and its real cost appears here
                </div>
              }
            >
              <For each={eco()!.metered}>
                {([name, m]) => (
                  <div class={s.pocketBlock}>
                    <div class={s.pocketHead}>
                      <span class={s.value}>{name}</span>
                      <span class={s.mono}>{m.runs} run{m.runs === 1 ? "" : "s"}</span>
                      <span class={s.value}>{m.meanPawa.toFixed(2)} pawa</span>
                    </div>
                    <div class={s.attentionWhy}>
                      compute {m.totalCompute} · storage {m.totalStorage}B · total{" "}
                      {m.totalPawa.toFixed(2)}
                    </div>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>ledger · newest first</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>{eco()?.entries.length ?? 0} shown</span>
          </header>
          <div class={s.cardBody}>
            <Show
              when={(eco()?.entries ?? []).length > 0}
              fallback={<div class={s.empty}>no entries</div>}
            >
              <For each={eco()!.entries}>
                {(x) => (
                  <div class={s.logRow}>
                    <span
                      class={s.seqCell}
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
                    <span>
                      <span style={{ color: vars.color.textPrimary }}>{fmt(x.amount)}</span>{" "}
                      <span class={s.sustainMeta}>
                        {x.principal}
                        {x.counterparty ? ` → ${x.counterparty}` : ""}
                      </span>
                      <br />
                      <span class={s.attentionWhy}>{x.authority}</span>
                    </span>
                    <span class={s.seqCell}>
                      {x.circulationDelta > 0 ? "+" : x.circulationDelta < 0 ? "−" : "±"}
                      {fmt(Math.abs(x.circulationDelta))}
                    </span>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </section>
      </div>
    </div>
  );
}
