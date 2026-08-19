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
import { engine, fmt } from "./lib/engine";
import {
  attentionAcross,
  hydrate,
  refreshIdentity,
  refreshWorld,
  subscribe,
  world,
} from "./lib/live";
import Constellation from "./screens/Constellation";
import Monitor from "./screens/Monitor";
import Console from "./screens/Console";
import Simulate from "./screens/Simulate";
import Composition from "./screens/Composition";
import Economy from "./screens/Economy";
import Define from "./screens/Define";
import Profile from "./screens/Profile";
import Lock from "./screens/Lock";
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

  /**
   * ★★★ Everything the cockpit needs, AFTER the identity is unlocked.
   *
   * Not a convenience: while locked there is no principal, so every command
   * that touches a Sustain would be refused anyway. Loading a world nobody may
   * act on would render a cockpit that looks alive and can do nothing.
   */
  const openCockpit = async () => {
    const stop = await subscribe();
    onCleanup(stop);
    await refreshWorld();
    await Promise.all(world.order.map((id) => hydrate(id)));
  };

  onMount(async () => {
    // The one thing a locked cockpit may ask for: who this machine is.
    const id = await refreshIdentity();
    if (id?.unlocked) await openCockpit();
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

  /**
   * ★ The household total for the belt — read from whichever Sustain declares
   * one, never assembled here. `undefined` means nothing declared a total,
   * which is a different fact from a total nobody could compute.
   */
  const householdTotal = () => {
    for (const id of world.order) {
      const agg = world.sustains[id]?.rollup?.aggregates.find(
        (a) => a.childPath === "finances.liquid.balance",
      );
      if (agg) return agg;
    }
    return undefined;
  };

  // ★★★ THE DOOR. Not a curtain: the host has no key while locked, so the gate
  //   refuses every call with `not_authenticated` whatever this renders.
  return (
    <Show
      when={world.identity?.unlocked}
      fallback={
        <Show when={world.identity} fallback={<div class={L.panelPad}><Empty>reading the local identity…</Empty></div>}>
          {(id) => (
            <Lock
              identity={id()}
              onUnlocked={() => {
                void refreshIdentity().then(() => void openCockpit());
              }}
            />
          )}
        </Show>
      }
    >
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
        {/* ★★ An AUTHENTICATED principal, and the key that proves it. The
            fingerprint is short on purpose — enough to notice if it ever
            changed, not so long it becomes furniture. */}
        <span class={`${S.meta} ${L.hideNarrow}`}>{world.principal || "—"}</span>
        <span class={`${S.caption} ${L.hideNarrow}`} title={world.identity?.publicKey ?? ""}>
          {world.identity?.publicKey ? `key ${world.identity.publicKey.slice(0, 8)}` : ""}
        </span>
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
        {/* ★★ Real now: the engine's roll-up ρ, pushed after every commit that
            could have moved it. ★ Still a dash when nothing declares a total or
            nothing was readable — the cells that cannot be honest stay dashes,
            and each says why. */}
        <span class={L.beltCell}>
          <span>household liquid</span>
          <Show
            when={householdTotal()?.grounded}
            fallback={<span class={L.beltAbsent}>{householdTotal() ? "— none readable" : "— none declared"}</span>}
          >
            <span class={L.beltValue}>{fmt(householdTotal()!.value)}</span>
          </Show>
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
    </Show>
  );
}
