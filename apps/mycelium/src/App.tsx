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
  noteWorldError,
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
import Orchie from "./screens/Orchie";
import { Council } from "./screens/Panels";
import { Library } from "./screens/Library";
import { Network } from "./screens/Network";
import Ingest from "./screens/Ingest";

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

/// Is this a touch-first device?
///
/// ★★ Asked of the DEVICE, not of the build. A Tauri Android build and a
/// phone browser hitting the dev server should behave the same way, and a
/// desktop window narrowed to phone width should not suddenly change face —
/// so this reads the pointer, which is the thing that actually differs, rather
/// than the viewport, which is the thing that merely correlates.
function isTouchFirst(): boolean {
  if (typeof window === "undefined" || !window.matchMedia) return false;
  return window.matchMedia("(pointer: coarse)").matches;
}

export default function App() {
  const [panel, setPanel] = createSignal<Panel>("constellation");
  /**
   * ★★★ One app, two faces. Mycelium is the cockpit; Orchie is the phone-first
   * curated surface. Same engine, same gate, same unlocked identity — a
   * different question, so a different arrangement of the same primitives.
   */
  /// ★★★ **On a phone the app opens to Orchie, and the toggle stays.** The
  /// two faces are one app, so this is a default rather than a fork: a
  /// touch-first device gets the curated surface first because that is the
  /// question it is usually being asked, and the cockpit is one tap away.
  /// Desktop opens to Mycelium for exactly the same reason in reverse.
  const [face, setFace] = createSignal<"mycelium" | "orchie">(
    isTouchFirst() ? "orchie" : "mycelium",
  );
  const [busy, setBusy] = createSignal(false);

  /**
   * ★★★ Everything the cockpit needs, AFTER the identity is unlocked.
   *
   * Not a convenience: while locked there is no principal, so every command
   * that touches a Sustain would be refused anyway. Loading a world nobody may
   * act on would render a cockpit that looks alive and can do nothing.
   */
  const openCockpit = async () => {
    // ★★★ **The world FIRST, and the order is the fix.**
    //
    // `subscribe()` used to come first and be awaited, so attaching the push
    // channel gated the one thing every surface needs -- which Sustain is
    // selected. On the phone Orchie mounts the instant the identity opens and
    // asks for the household immediately, so anything slow or broken ahead of
    // `refreshWorld` is a stall the person sees and nothing else explains.
    // Nothing here depends on the subscription having attached.
    await refreshWorld();

    // ★★ Live updates are an enhancement, not a precondition. If the channel
    //    will not attach, the household still opens and the failure is SAID
    //    rather than swallowed by a bare `void`.
    try {
      const stop = await subscribe();
      onCleanup(stop);
    } catch (e) {
      noteWorldError(`live updates unavailable — ${String(e)}`);
    }

    // ★ One unreadable Sustain must not take the other six down with it.
    await Promise.all(
      world.order.map((id) =>
        hydrate(id).catch((e) => noteWorldError(`could not read ${id} — ${String(e)}`)),
      ),
    );
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
              /* ★ The door wears the face that is asking. On a phone that is
                 Orchie's, because this is the first screen anyone sees and it
                 was the cockpit's technical briefing for everybody. */
              face={face()}
              onUnlocked={() => {
                // ★ A bare `void` on both halves of this chain is how a failure
                //   right after unlock became an unexplained empty screen.
                void refreshIdentity()
                  .then(() => openCockpit())
                  .catch((e) => noteWorldError(`could not open the household — ${String(e)}`));
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

        <button
          class={S.chip}
          onClick={() => setFace((f) => (f === "mycelium" ? "orchie" : "mycelium"))}
        >
          {face() === "mycelium" ? "orchie" : "mycelium"}
        </button>
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

      <Show when={face() === "orchie"}>
        <Orchie />
      </Show>
      <div class={L.body} style={{ display: face() === "orchie" ? "none" : undefined }}>
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
