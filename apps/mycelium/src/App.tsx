/**
 * MYCELIUM — V1.4. The app shell.
 *
 * ★★ The frame is permanent and the panel swaps inside it: left nav, topbar
 * with the Sustain selector, status belt. **One selection model** — the
 * selector, the nav and the constellation all read and write `world.selected`,
 * so they cannot disagree about what you are looking at.
 *
 * ★ Panels the roadmap names but the app does not have yet are listed and
 * **disabled**, marked `soon`. A nav that hid them would misrepresent the
 * product; one that pretended they worked would be worse.
 */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import * as s from "./styles/app.css";
import { vars } from "./styles/tokens.css";
import { engine, fmt } from "./lib/engine";
import { attentionAcross, hydrate, refreshWorld, selectedSustain, subscribe, world } from "./lib/live";
import Constellation from "./screens/Constellation";
import Monitor from "./screens/Monitor";
import Console from "./screens/Console";
import Simulate from "./screens/Simulate";
import Composition from "./screens/Composition";
import Economy from "./screens/Economy";

type Panel = "constellation" | "monitor" | "console" | "simulate" | "composition" | "economy";

/** The roadmap's panel list. `soon` ones are shown, disabled, and labelled. */
const NAV: { group: string; items: { id: Panel | string; label: string; soon?: boolean }[] }[] = [
  {
    group: "watch",
    items: [
      { id: "constellation", label: "Constellation" },
      { id: "monitor", label: "Monitor" },
      { id: "council", label: "Council", soon: true },
    ],
  },
  {
    group: "act",
    items: [
      { id: "console", label: "Console" },
      { id: "simulate", label: "Simulate" },
      { id: "define", label: "Define", soon: true },
      { id: "composition", label: "Composition" },
    ],
  },
  {
    group: "system",
    items: [
      { id: "economy", label: "Economy" },
      { id: "ingest", label: "Ingest", soon: true },
      { id: "network", label: "Network", soon: true },
      { id: "library", label: "Library", soon: true },
      { id: "profile", label: "Profile", soon: true },
    ],
  },
];

function Clock() {
  const [now, setNow] = createSignal(new Date());
  // ★ The UI's own clock, for the person reading the screen. The ENGINE has
  //   none, and nothing here feeds it — no timestamp on this screen is ever
  //   attributed to the engine.
  const t = setInterval(() => setNow(new Date()), 1000);
  onCleanup(() => clearInterval(t));
  return (
    <span class={s.brandSub}>
      {now().toLocaleTimeString(undefined, { hour12: false })} local
    </span>
  );
}

export default function App() {
  const [panel, setPanel] = createSignal<Panel>("constellation");
  const [busy, setBusy] = createSignal(false);

  onMount(async () => {
    // ★★★ ONE subscription for the whole app. Every screen reads the store it
    //   fills; none of them listens on its own, and none of them polls.
    const stop = await subscribe();
    onCleanup(stop);

    await refreshWorld();
    await Promise.all(world.order.map((id) => hydrate(id)));
  });

  /** ★★ The single selection path. Everything that changes the subject calls this. */
  const select = async (id: string) => {
    if (!id || id === world.selected) return;
    setBusy(true);
    try {
      await engine.select(id);
      await refreshWorld();
      await hydrate(id);
    } finally {
      setBusy(false);
    }
  };

  /** Drill-in: pick the Sustain AND open Monitor on it. */
  const open = async (id: string) => {
    await select(id);
    setPanel("monitor");
  };

  const attentionCount = () => attentionAcross().length;

  return (
    <div class={s.frame}>
      {/* ── topbar ────────────────────────────────────────────────────────── */}
      <header class={s.topbar}>
        <span class={s.brand}>Mycelium</span>
        <span class={s.brandSub}>v1.4</span>

        <select
          class={s.picker}
          value={world.selected ?? ""}
          disabled={busy() || world.order.length === 0}
          onChange={(e) => void select(e.currentTarget.value)}
        >
          <For each={world.order}>
            {(id) => {
              const su = world.sustains[id];
              return (
                <option value={id}>
                  {su?.summary.parent ? "· " : ""}
                  {su?.summary.label ?? id}
                </option>
              );
            }}
          </For>
        </select>

        <span class={s.spacer} />

        <span class={s.pushBadge}>{world.pushes} pushed</span>
        <Show when={world.refusals > 0}>
          <span class={s.refusedBadge}>{world.refusals} refused</span>
        </Show>

        <span class={s.avatar}>bg</span>
        <span class={s.identity}>{world.principal || "—"}</span>
        <Clock />
      </header>

      {/* ── body ──────────────────────────────────────────────────────────── */}
      <div class={s.body}>
        <nav class={s.nav}>
          <For each={NAV}>
            {(g) => (
              <div class={s.navGroup}>
                <span class={s.navGroupTitle}>{g.group}</span>
                <For each={g.items}>
                  {(item) => (
                    <button
                      class={panel() === item.id ? s.navItemActive : s.navItem}
                      disabled={item.soon}
                      onClick={() => !item.soon && setPanel(item.id as Panel)}
                    >
                      {item.label}
                      <Show when={item.id === "constellation" && attentionCount() > 0}>
                        <span class={s.navSoon} style={{ color: vars.color.warn }}>
                          {attentionCount()}
                        </span>
                      </Show>
                      <Show when={item.soon}>
                        <span class={s.navSoon}>soon</span>
                      </Show>
                    </button>
                  )}
                </For>
              </div>
            )}
          </For>

          <div class={s.invariantRow} style={{ "margin-top": "auto" }}>
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
          <Show when={world.loaded} fallback={<div class={s.empty} style={{ padding: vars.space.xl }}>
            {world.error ? `engine unreachable — ${world.error}` : "opening the household…"}
          </div>}>
            <Show when={panel() === "constellation"}>
              <Constellation onOpen={(id) => void open(id)} />
            </Show>
            <Show when={panel() === "monitor"}>
              <Monitor />
            </Show>
            <Show when={panel() === "console"}>
              <Console />
            </Show>
            <Show when={panel() === "simulate"}>
              <Simulate />
            </Show>
            <Show when={panel() === "composition"}>
              <Composition />
            </Show>
            <Show when={panel() === "economy"}>
              <Economy />
            </Show>
          </Show>
        </div>
      </div>

      {/* ── status belt ───────────────────────────────────────────────────── */}
      <footer class={s.statusBelt}>
        <span class={s.beltCell}>
          <span>engine</span>
          <span class={s.beltValue}>sustena-core · embedded</span>
        </span>
        <span class={s.beltCell}>
          <span>host</span>
          <span class={s.beltValue}>{world.storePath || "—"}</span>
        </span>
        <span class={s.beltCell}>
          <span>sustains</span>
          <span class={s.beltValue}>{world.order.length}</span>
        </span>
        <span class={s.beltCell}>
          <span>liquid (household)</span>
          {/* ★ Not summed — roll-up ρ is not in the core. */}
          <span class={s.beltAbsent}>— needs ρ</span>
        </span>
        <span class={s.beltCell}>
          <span>peers</span>
          {/* ★ No multi-node host exists. An honest dash, not a zero. */}
          <span class={s.beltAbsent}>— single node</span>
        </span>
        <span class={s.beltCell}>
          <span>pawa</span>
          {/* ★ The economy is built in the engine and not wired into this host. */}
          <span class={s.beltAbsent}>— not metered here</span>
        </span>
        <span class={s.spacer} />
        <span>push channel · one message per gate decision · no polling</span>
      </footer>
    </div>
  );
}
