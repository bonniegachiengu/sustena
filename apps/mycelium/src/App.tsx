/**
 * MYCELIUM — V1.2.
 *
 * The shell: a household selector, two screens, and **one subscription to the
 * push channel** that both of them read from. There is no polling anywhere in
 * this app.
 */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import * as s from "./styles/app.css";
import { vars } from "./styles/tokens.css";
import { fmt, type SustainSummary } from "./lib/engine";
import { engine } from "./lib/engine";
import { childrenOf, hydrate, refreshWorld, selectedSustain, subscribe, world } from "./lib/live";
import Monitor from "./screens/Monitor";
import Console from "./screens/Console";

type Screen = "monitor" | "console";

export default function App() {
  const [screen, setScreen] = createSignal<Screen>("monitor");
  const [busy, setBusy] = createSignal(false);

  onMount(async () => {
    // ★★★ ONE subscription for the whole app. Every screen reads the store it
    //   fills; none of them listens on its own, and none of them polls.
    const stop = await subscribe();
    onCleanup(stop);

    await refreshWorld();
    // Hydrate every Sustain once — the parts a push does not carry (the log,
    // and state for Sustains we have not touched yet).
    await Promise.all(world.order.map((id) => hydrate(id)));
  });

  const pick = async (id: string) => {
    setBusy(true);
    try {
      await engine.select(id);
      await refreshWorld();
      await hydrate(id);
    } finally {
      setBusy(false);
    }
  };

  const roots = () =>
    world.order.map((k) => world.sustains[k]!).filter((x) => x && x.summary.parent === null);

  const Row = (props: { row: SustainSummary; child?: boolean }) => (
    <button
      class={`${world.selected === props.row.id ? s.sustainRowActive : s.sustainRow} ${
        props.child ? s.childIndent : ""
      }`}
      onClick={() => void pick(props.row.id)}
      disabled={busy()}
    >
      <span
        class={s.dot}
        style={{
          background: world.selected === props.row.id ? vars.color.amber : vars.color.borderLight,
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
        <span class={s.brandSub}>v1.2 · live</span>

        <div class={s.tabs} style={{ "margin-left": vars.space.lg }}>
          <button
            class={screen() === "monitor" ? s.tabActive : s.tab}
            onClick={() => setScreen("monitor")}
          >
            monitor
          </button>
          <button
            class={screen() === "console" ? s.tabActive : s.tab}
            onClick={() => setScreen("console")}
          >
            console
          </button>
        </div>

        <span class={s.spacer} />

        {/* ★ The push counter. Not decoration — it is the number that says the
            channel is live, and it only moves when the engine commits. */}
        <span class={s.pushBadge}>{world.pushes} pushed</span>
        <Show when={selectedSustain()}>
          {(l) => <span class={s.brandSub}>· {l().summary.label}</span>}
        </Show>
      </header>

      <div style={{ display: "grid", "grid-template-columns": "260px 1fr", "min-height": 0 }}>
        {/* ── the household ────────────────────────────────────────────── */}
        <nav
          class={s.column}
          style={{
            padding: vars.space.md,
            "border-right": `1px solid ${vars.color.border}`,
            background: vars.color.bgSurface,
          }}
        >
          <span class={s.cardTitle}>the household</span>
          <Show
            when={world.loaded}
            fallback={
              <div class={s.empty}>{world.error ? "engine unreachable" : "opening…"}</div>
            }
          >
            <Show
              when={world.order.length > 0}
              fallback={<div class={s.empty}>no sustains yet · the store is empty</div>}
            >
              <div class={s.selector}>
                <For each={roots()}>
                  {(root) => (
                    <>
                      <Row row={root.summary} />
                      <For each={childrenOf(root.summary.id)}>
                        {(kid) => <Row row={kid} child />}
                      </For>
                    </>
                  )}
                </For>
              </div>
            </Show>
          </Show>

          <div class={s.invariantRow} style={{ "margin-top": vars.space.md }}>
            <span
              class={s.dot}
              style={{
                background: world.holds ? vars.color.teal : vars.color.danger,
                "margin-top": "5px",
              }}
            />
            <span>
              <Show
                when={world.holds}
                fallback={<span style={{ color: vars.color.danger }}>{world.holarchyReason}</span>}
              >
                ⊕ holds · {world.linked} linked
                <br />
                <span class={s.attentionWhy}>checked by the engine</span>
              </Show>
            </span>
          </div>
        </nav>

        <div style={{ "min-width": 0, "min-height": 0, overflow: "hidden", display: "grid" }}>
          <Show when={screen() === "monitor"} fallback={<Console />}>
            <Monitor />
          </Show>
        </div>
      </div>

      <footer class={s.statusBelt}>
        <span>store</span>
        <span style={{ color: vars.color.textSecondary }}>{world.storePath || "—"}</span>
        <span class={s.spacer} />
        <span>push channel · one message per committed change · no polling</span>
      </footer>
    </div>
  );
}
