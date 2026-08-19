/**
 * MYCELIUM — v1. The app shell.
 *
 * ★★ The frame is permanent and the panel swaps inside it: a nav rail, a topbar
 * with the Sustain selector, and a status belt. **One selection model** — the
 * selector, the nav and the constellation all read and write `world.selected`,
 * so they cannot disagree about what you are looking at.
 *
 * ★★★ Every panel is navigable. Three of them describe a subsystem
 * `sustena-core` does not have, and say what exists, what does not, and where
 * the capability lives — in the cockpit's own layout, at the cockpit's own
 * fidelity. A disabled item told a person nothing.
 */
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import * as L from "./ui/layout.css";
import * as S from "./ui/ui.css";
import { Badge, Caption, Empty, ErrorState, NoteRow } from "./ui";
import { engine } from "./lib/engine";
import { attentionAcross, hydrate, refreshWorld, subscribe, world } from "./lib/live";
import Constellation from "./screens/Constellation";
import Monitor from "./screens/Monitor";
import Console from "./screens/Console";
import Simulate from "./screens/Simulate";
import Composition from "./screens/Composition";
import Economy from "./screens/Economy";
import Define from "./screens/Define";
import Profile from "./screens/Profile";
import { Council, Ingest, Library, Network } from "./screens/Panels";

type Panel =
  | "constellation" | "monitor" | "council"
  | "console" | "simulate" | "define" | "composition"
  | "economy" | "ingest" | "network" | "library" | "profile";

const NAV: { group: string; items: { id: Panel; label: string }[] }[] = [
  {
    group: "watch",
    items: [
      { id: "constellation", label: "Constellation" },
      { id: "monitor", label: "Monitor" },
      { id: "council", label: "Council" },
    ],
  },
  {
    group: "act",
    items: [
      { id: "console", label: "Console" },
      { id: "simulate", label: "Simulate" },
      { id: "define", label: "Define" },
      { id: "composition", label: "Composition" },
    ],
  },
  {
    group: "system",
    items: [
      { id: "economy", label: "Economy" },
      { id: "ingest", label: "Ingest" },
      { id: "network", label: "Network" },
      { id: "library", label: "Library" },
      { id: "profile", label: "Profile" },
    ],
  },
];

function Clock() {
  const [now, setNow] = createSignal(new Date());
  // ★ The UI's own clock, for the person reading the screen. The ENGINE has
  //   none, and nothing here feeds it — no timestamp in this app is ever
  //   attributed to the engine.
  const t = setInterval(() => setNow(new Date()), 1000);
  onCleanup(() => clearInterval(t));
  return <span class={S.caption}>{now().toLocaleTimeString(undefined, { hour12: false })} local</span>;
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
    <div class={L.frame}>
      <header class={L.topbar}>
        <span class={S.brand}>MYCELIUM</span>
        <span class={`${S.caption} ${L.hideNarrow}`}>v1</span>

        <select
          class={S.select}
          style={{ "max-width": "200px", width: "auto" }}
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

        <span class={S.spacer} />

        <Badge tone="ok">{world.pushes} pushed</Badge>
        <Show when={world.refusals > 0}>
          <Badge tone="danger">{world.refusals} refused</Badge>
        </Show>

        <span class={S.avatar}>bg</span>
        <span class={`${S.meta} ${L.hideNarrow}`}>{world.principal || "—"}</span>
        <span class={L.hideNarrow}>
          <Clock />
        </span>
      </header>

      <div class={L.body}>
        <nav class={L.nav}>
          <For each={NAV}>
            {(g) => (
              <div class={L.navGroup}>
                <span class={L.navGroupTitle}>{g.group}</span>
                <For each={g.items}>
                  {(item) => (
                    <button
                      class={panel() === item.id ? L.navItem.active : L.navItem.idle}
                      onClick={() => setPanel(item.id)}
                    >
                      {item.label}
                      <Show when={item.id === "constellation" && attentionCount() > 0}>
                        <span class={L.navCountLive}>
                          {attentionCount()}
                        </span>
                      </Show>
                    </button>
                  )}
                </For>
              </div>
            )}
          </For>

          <div class={L.navFooter}>
            <NoteRow tone={world.holds ? "ok" : "danger"}>
              <Show when={world.holds} fallback={<Caption>{world.holarchyReason}</Caption>}>
                <Caption>
                  ⊕ holds · {world.linked} linked
                  <br />
                  checked by the engine
                </Caption>
              </Show>
            </NoteRow>
          </div>
        </nav>

        <div class={L.panelSlot}>
          <Show
            when={world.loaded}
            fallback={
              <div class={L.panelPad}>
                <Show
                  when={world.error}
                  fallback={<Empty>opening the household · folding every log</Empty>}
                >
                  {(e) => (
                    <ErrorState>
                      the engine is unreachable — {e()}
                      <br />
                      nothing is shown rather than something stale.
                    </ErrorState>
                  )}
                </Show>
              </div>
            }
          >
            <Show when={panel() === "constellation"}><Constellation onOpen={(id) => void open(id)} /></Show>
            <Show when={panel() === "monitor"}><Monitor /></Show>
            <Show when={panel() === "council"}><Council /></Show>
            <Show when={panel() === "console"}><Console /></Show>
            <Show when={panel() === "simulate"}><Simulate /></Show>
            <Show when={panel() === "define"}><Define /></Show>
            <Show when={panel() === "composition"}><Composition /></Show>
            <Show when={panel() === "economy"}><Economy /></Show>
            <Show when={panel() === "ingest"}><Ingest /></Show>
            <Show when={panel() === "network"}><Network /></Show>
            <Show when={panel() === "library"}><Library /></Show>
            <Show when={panel() === "profile"}><Profile /></Show>
          </Show>
        </div>
      </div>

      <footer class={L.statusBelt}>
        <span class={L.beltCell}>
          <span>engine</span>
          <span class={L.beltValue}>sustena-core · embedded</span>
        </span>
        <span class={`${L.beltCell} ${L.hideNarrow}`}>
          <span>host</span>
          <span class={L.beltValue}>{world.storePath || "—"}</span>
        </span>
        <span class={L.beltCell}>
          <span>sustains</span>
          <span class={L.beltValue}>{world.order.length}</span>
        </span>
        {/* ★ Cells that cannot be honest are dashes, and each says why. */}
        <span class={L.beltCell}>
          <span>household liquid</span>
          <span class={L.beltAbsent}>— needs ρ</span>
        </span>
        <span class={L.beltCell}>
          <span>peers</span>
          <span class={L.beltAbsent}>— single node</span>
        </span>
        <span class={L.beltCell}>
          <span>pawa</span>
          <span class={L.beltValue}>metered · charged</span>
        </span>
        <span class={S.spacer} />
        <span class={L.hideNarrow}>push channel · one message per gate decision · no polling</span>
      </footer>
    </div>
  );
}
