/**
 * COMPOSITION — `⊕`, and two honest absences.
 *
 * ★★ The tree is **real and engine-checked**: `MonitorEngine::flatten_holarchy`
 * refuses a duplicate id, an unknown parent and a cycle.
 *
 * ★★★ Two capabilities the Python engine has and `sustena-core` does not, both
 * rendered as honest unavailable rather than approximated:
 *
 * - **roll-up ρ** — folding children into a parent aggregate.
 * - **`holon.transfer`** — an atomic, conserved move between two Sustains.
 *
 * The second is the more dangerous one to fake, and the screen says why.
 */
import { createMemo, For, Show } from "solid-js";
import * as s from "../styles/app.css";
import { vars } from "../styles/tokens.css";
import { fmt } from "../lib/engine";
import { childrenOf, selectedSustain, world } from "../lib/live";

export default function Composition() {
  const live = () => selectedSustain();
  const kids = createMemo(() => (live() ? childrenOf(live()!.summary.id) : []));
  const parent = createMemo(() => {
    const p = live()?.summary.parent;
    return p ? world.sustains[p]?.summary : undefined;
  });

  return (
    <div class={s.main}>
      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>⊕ · the tree</span>
            <span class={s.spacer} />
            <span class={s.sustainMeta}>{live()?.summary.label ?? "—"}</span>
          </header>
          <div class={s.cardBody}>
            <div class={s.invariantRow}>
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
                  <strong style={{ color: vars.color.textPrimary }}>tree holds</strong> ·{" "}
                  {world.linked} linked
                  <br />
                  <span class={s.attentionWhy}>
                    validated by <code>MonitorEngine::flatten_holarchy</code> — no duplicate id, no
                    unknown parent, no cycle
                  </span>
                </Show>
              </span>
            </div>

            <div class={s.label} style={{ "margin-top": vars.space.lg }}>parent</div>
            <Show when={parent()} fallback={<div class={s.empty}>none · this is a root Sustain</div>}>
              {(p) => (
                <div class={s.pocketRow}>
                  <span class={s.value}>{p().label}</span>
                  <span class={s.mono}>{p().template}</span>
                  <span class={s.mono}>{p().events} events</span>
                  <span class={s.value}>{fmt(p().liquid)}</span>
                </div>
              )}
            </Show>

            <div class={s.label} style={{ "margin-top": vars.space.lg }}>
              children · {kids().length}
            </div>
            <Show when={kids().length > 0} fallback={<div class={s.empty}>no children linked</div>}>
              <For each={kids()}>
                {(k) => (
                  <div class={s.pocketRow}>
                    <span class={s.value}>{k.label}</span>
                    <span class={s.mono}>{k.template}</span>
                    <span class={s.mono}>{k.events} events</span>
                    <span class={s.value}>{fmt(k.liquid)}</span>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </section>
      </div>

      <div class={s.column}>
        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>roll-up ρ</span>
          </header>
          <div class={s.cardBody}>
            <div class={s.unavailable}>
              <span class={s.unavailableTitle}>not available</span>
              The engine holds and checks this tree, but it cannot yet fold a child's state into a
              parent aggregate. ρ exists in the Python engine and has not been ported to{" "}
              <code>sustena-core</code>.
              <br />
              <br />
              Nothing is summed. A household total computed by this app rather than by the engine
              would be a number with no rule behind it — and no way to tell you when it stopped
              being true.
            </div>
          </div>
        </section>

        <section class={s.card}>
          <header class={s.cardHead}>
            <span class={s.cardTitle}>transfer between Sustains</span>
          </header>
          <div class={s.cardBody}>
            <div class={s.unavailable}>
              <span class={s.unavailableTitle}>not available</span>
              <code>holon.transfer</code> — an <strong>atomic, conserved</strong> move that debits
              one Sustain's pocket and credits another's, both committed together or neither — is
              in the Python engine and <strong>not in <code>sustena-core</code></strong>.
              <br />
              <br />
              ★★★ <strong>This one is deliberately not approximated.</strong> The obvious host
              workaround — spend from one Sustain, then record income on the other — is two
              separate gated calls. If the second refuses, the money has left one household and
              arrived nowhere. That is not a transfer; it is a way to lose money that looks like a
              feature.
              <br />
              <br />
              <span class={s.attentionWhy}>
                <code>juul::transfer</code> does exist in the core — but that moves the{" "}
                <em>economy's internal unit</em> between principals, not a household's own money
                between Sustains. Using it here would be answering a different question.
              </span>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
