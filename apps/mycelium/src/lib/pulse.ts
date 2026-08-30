/**
 * The relayed, refractory pulse — Multiparty §III.
 *
 * ★★★ **What this is for.** Some screens read the live world store and so
 * re-render on their own; some ask the engine a question once and then show the
 * answer forever. The second kind is where "close it and reopen it" comes from.
 * A household changed — locally, or because a peer wrote into it — and nothing
 * told the screen.
 *
 * §III is the article's own mechanism and not a UI convention borrowed from
 * elsewhere: *"each element rests, fires when driven past threshold, then goes
 * briefly refractory so the excitation moves outward instead of sloshing back
 * into the elements that just fired."* Here the elements are the surfaces. A
 * state change fires one pulse, every subscribed surface refreshes, and the
 * refractory window is what stops a refresh from being read as another change.
 *
 * ★★ **The refractory term is load-bearing, exactly as the article says.**
 * Without it a burst of arriving entries would queue one refetch per entry, and
 * a screen mid-refetch would start another. `RESTING → EXCITED → REFRACTORY`:
 * while a refresh is in flight, further pulses are remembered as one, and
 * fire once when it settles. That is why this is a state machine and not a
 * `createEffect` on a counter.
 */

import { createEffect, createSignal, onCleanup } from "solid-js";

/** How long a surface stays refractory after refreshing. */
const REFRACTORY_MS = 250;

const [pulses, setPulses] = createSignal(0);

/**
 * Relay a pulse: something changed.
 *
 * ★ Called from `live.ts` where the engine's own messages arrive, so there is
 * exactly one origin and no surface has to know which kind of change it was.
 */
export function relay(): void {
  setPulses((n) => n + 1);
}

/** How many pulses have been relayed. For a surface that wants to show it. */
export const pulseCount = pulses;

type Phase = "resting" | "excited" | "refractory";

/**
 * Refresh when a pulse arrives, at most once per refractory window.
 *
 * Hand it the `refetch` a `createResource` already gave you:
 *
 * ```ts
 * const [net, { refetch }] = createResource(() => engine.network());
 * onPulse(refetch);
 * ```
 */
export function onPulse(refresh: () => unknown | Promise<unknown>): void {
  let phase: Phase = "resting";
  let missed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;

  const fire = () => {
    if (disposed) return;
    phase = "excited";
    void Promise.resolve(refresh())
      .catch(() => {
        // ★ A failed refresh must not leave the surface permanently
        //   refractory: it would go quiet forever after one bad moment.
      })
      .finally(() => {
        if (disposed) return;
        phase = "refractory";
        timer = setTimeout(() => {
          phase = "resting";
          if (missed) {
            missed = false;
            fire();
          }
        }, REFRACTORY_MS);
      });
  };

  createEffect(() => {
    // Subscribe. The first run is the mount, and the resource has already
    // fetched, so nothing is fired for it.
    const n = pulses();
    if (n === 0) return;
    if (phase === "resting") {
      fire();
    } else {
      // ★ Remembered as ONE, not queued as many. A hundred arriving entries
      //   are a hundred pulses and exactly one extra refresh.
      missed = true;
    }
  });

  onCleanup(() => {
    disposed = true;
    if (timer) clearTimeout(timer);
  });
}
