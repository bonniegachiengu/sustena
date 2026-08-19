/**
 * PROFILE — identity, and the **real** capability model.
 *
 * ★★ The permission matrix is the engine's: `effective_privilege` walks the
 * membership path with the weakest-link rule, and `permitted` compares it
 * against each operator's declared `min_privilege`.
 *
 * ★★★ **And the gate is not checking any of it.** Every call in this host runs
 * `Authorization::Unchecked`. This screen shows the authority the principal
 * *holds*, not one being enforced — a permission matrix that implied
 * enforcement would be security theatre, so it says so at the top.
 */
import { createResource, For, Show } from "solid-js";
import * as s from "../styles/app.css";
import { vars } from "../styles/tokens.css";
import { engine } from "../lib/engine";
import { selectedSustain, world } from "../lib/live";

const TIER = ["owner", "member", "contributor", "observer"];
const tierName = (t: number | null | undefined) =>
  t === null || t === undefined ? "—" : (TIER[t] ?? `tier ${t}`);

export default function Profile() {
  const [access] = createResource(
    () => world.selected,
    (id) => engine.access(id),
  );

  const totalEvents = () =>
    world.order.reduce((n, k) => n + (world.sustains[k]?.summary.events ?? 0), 0);

  return (
    <div class={s.main}>
      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>identity</span>
          </header>
          <div class={s.cardBody}>
            <div style={{ display: "flex", gap: vars.space.md, "align-items": "center" }}>
              <span class={s.avatarLarge}>bg</span>
              <span>
                <span class={s.valueBig}>{world.principal || "—"}</span>
                <br />
                <span class={s.attentionWhy}>the local principal</span>
              </span>
            </div>

            {/* ★★★ The honesty that matters most on this screen. */}
            <div class={s.unavailable} style={{ "margin-top": vars.space.lg }}>
              <span class={s.unavailableTitle}>declared, not authenticated</span>
              There is no sign-in. This handle is a name the host declares and the ledger charges
              to — it is not proof of anything, and nothing checked it.
            </div>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>activity</span>
          </header>
          <div class={s.cardBody}>
            <div class={s.telemetry}>
              <div class={s.telemetryCell}>
                <span class={s.label}>sustains</span>
                <span class={s.value}>{world.order.length}</span>
              </div>
              <div class={s.telemetryCell}>
                <span class={s.label}>logged events</span>
                <span class={s.value}>{totalEvents()}</span>
              </div>
              <div class={s.telemetryCell}>
                <span class={s.label}>pushed this session</span>
                <span class={s.value}>{world.pushes}</span>
              </div>
              <div class={s.telemetryCell}>
                <span class={s.label}>refused this session</span>
                <span class={s.value}>{world.refusals}</span>
              </div>
            </div>
            <div class={s.attentionWhy}>
              Every figure is counted from the persisted world or this session's channel. Nothing
              here is a running total the app keeps on its own.
            </div>
          </div>
        </section>
      </div>

      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>access · {selectedSustain()?.summary.label ?? "—"}</span>
            <span class={s.spacer} />
            <Show when={access()}>
              {(a) => (
                <span class={a().enforced ? s.pushBadge : s.unavailableBadge}>
                  {a().enforced ? "enforced" : "not enforced"}
                </span>
              )}
            </Show>
          </header>
          <div class={s.cardBody}>
            <Show when={access()} fallback={<div class={s.empty}>reading the capability model…</div>}>
              {(a) => (
                <>
                  <div class={s.row}>
                    <span class={s.label}>effective privilege</span>
                    <span class={s.value}>
                      {tierName(a().tier)}
                      {a().tier !== null ? ` · tier ${a().tier}` : ""}
                    </span>
                  </div>
                  <div class={s.row}>
                    <span class={s.label}>membership edges</span>
                    <span class={s.value}>{a().memberships}</span>
                  </div>
                  <div class={s.attentionWhy}>
                    Lower is more privileged: 0 owner, 1 member, 2 contributor, 3 observer. The
                    effective tier is the <strong>weakest link</strong> along the membership path,
                    computed by `effective_privilege` — not read off a single edge.
                  </div>

                  <div class={s.label} style={{ "margin-top": vars.space.lg }}>
                    what this principal may run
                  </div>
                  <For each={a().operators} fallback={<div class={s.empty}>no operators declared</div>}>
                    {(o) => (
                      <div class={s.pocketRow}>
                        <span class={s.value}>{o.operator}</span>
                        <span class={s.mono}>needs {tierName(o.requiredTier)}</span>
                        <span class={s.spacer} />
                        <span
                          style={{
                            color: o.permitted ? vars.color.teal : vars.color.danger,
                            "font-family": vars.font.mono,
                            "font-size": "10.5px",
                          }}
                        >
                          {o.permitted ? "permitted" : (o.denial ?? "denied")}
                        </span>
                      </div>
                    )}
                  </For>

                  <div class={s.unavailable} style={{ "margin-top": vars.space.lg }}>
                    <span class={s.unavailableTitle}>displayed, not enforced</span>
                    {a().note}
                  </div>
                </>
              )}
            </Show>
          </div>
        </section>
      </div>
    </div>
  );
}
