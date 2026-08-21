/**
 * THE ON-SCREEN KEYBOARD, AND THE PART OF THE SCREEN YOU CAN ACTUALLY SEE.
 *
 * ★★★ **This is the thing that was missing entirely.** Nothing in the app knew
 * the keyboard existed. It opened, it covered whatever input had focus, and the
 * layout never moved -- so on a phone you typed into a field you could not see.
 *
 * ★★ **Why `visualViewport` and not a resize listener.** On Android the window
 * is often NOT resized when the keyboard opens; what changes is the *visual*
 * viewport -- the part of the layout viewport actually on screen. `window.
 * innerHeight` can stay exactly the same while half the screen is covered.
 * `visualViewport.height` is the only number that tells the truth, and the
 * difference between it and the layout height IS the keyboard.
 *
 * ★★ **Why a CSS variable rather than inline styles.** The keyboard height is
 * needed by whatever happens to be at the bottom of the screen, which is a
 * layout concern, not a component's. Publishing `--kb` on the root element lets
 * a stylesheet reserve space with `padding-bottom: var(--kb)` and lets the
 * safe-area inset be combined with it in one expression. One writer, many
 * readers, and no component has to thread a pixel count through props.
 *
 * ★ Everything degrades: with no `visualViewport` (older WebViews, and the
 * desktop build) `--kb` stays `0px` and every consumer behaves as it always
 * did. Nothing here is required for the app to work; it is required for the app
 * to be usable with one thumb.
 */
import { createSignal, onCleanup } from "solid-js";

/** Height in px of the on-screen keyboard, or 0 when it is closed. */
const [keyboard, setKeyboard] = createSignal(0);
export { keyboard };

/**
 * A keyboard smaller than this is not a keyboard -- it is the URL bar
 * collapsing, a scroll-driven chrome change, or rounding. Acting on those makes
 * the layout twitch while someone is just scrolling.
 */
const NOISE_FLOOR = 120;

let started = false;

/**
 * Start publishing the keyboard height. Idempotent, so it is safe to call from
 * more than one screen; the first call wins and later ones are no-ops.
 */
export function watchViewport(): void {
  if (started) return;
  started = true;

  const vv = window.visualViewport;
  const root = document.documentElement;

  const publish = (px: number) => {
    setKeyboard(px);
    root.style.setProperty("--kb", `${px}px`);
    // A boolean hook for styles that need to do something other than reserve
    // space -- hiding a decorative header while typing, for instance.
    root.dataset.keyboard = px > 0 ? "open" : "closed";
  };

  if (!vv) {
    publish(0);
    return;
  }

  const measure = () => {
    // The layout viewport minus what is actually visible, minus how far the
    // visual viewport has been scrolled within it. offsetTop matters: when the
    // page is pushed up rather than resized, the difference shows up there.
    const hidden = root.clientHeight - vv.height - vv.offsetTop;
    publish(hidden > NOISE_FLOOR ? Math.round(hidden) : 0);
  };

  vv.addEventListener("resize", measure);
  vv.addEventListener("scroll", measure);
  measure();

  onCleanup(() => {
    vv.removeEventListener("resize", measure);
    vv.removeEventListener("scroll", measure);
    started = false;
  });
}

/**
 * Keep a focused field visible above the keyboard.
 *
 * ★★★ `scrollIntoView` alone is not enough and it is worth saying why: the
 * browser scrolls the element into the *layout* viewport, which on a phone
 * includes the region the keyboard is sitting on top of. The field ends up
 * "visible" by the browser's reckoning and invisible to the person. So this
 * waits for the keyboard to actually be up, then checks the element against the
 * VISUAL viewport and scrolls by the real shortfall.
 *
 * ★★ The delay is not a magic number for its own sake -- the keyboard animates
 * in, and measuring during that animation measures a keyboard that is halfway
 * up. Two passes: one soon, for responsiveness, one after the animation, for
 * correctness. Being right twice is cheaper than being wrong once.
 */
export function keepVisible(el: HTMLElement): void {
  const nudge = () => {
    const vv = window.visualViewport;
    const rect = el.getBoundingClientRect();

    // Where the bottom of the usable area actually is.
    const usableBottom = vv ? vv.height + vv.offsetTop : window.innerHeight;

    // A field flush against the keyboard is technically visible and feels
    // wrong; leave room for it to breathe and for its own label.
    const margin = 24;
    const overshoot = rect.bottom + margin - usableBottom;
    const undershoot = rect.top - margin;

    if (overshoot > 0) {
      scrollBy(el, overshoot);
    } else if (undershoot < 0) {
      scrollBy(el, undershoot);
    }
  };

  requestAnimationFrame(nudge);
  window.setTimeout(nudge, 320);
}

/** Scroll the nearest scrollable ancestor, falling back to the window. */
function scrollBy(el: HTMLElement, delta: number): void {
  const scroller = nearestScroller(el);
  if (scroller) {
    scroller.scrollTo({ top: scroller.scrollTop + delta, behavior: "smooth" });
  } else {
    window.scrollBy({ top: delta, behavior: "smooth" });
  }
}

function nearestScroller(el: HTMLElement): HTMLElement | null {
  let node: HTMLElement | null = el.parentElement;
  while (node) {
    const overflow = getComputedStyle(node).overflowY;
    if ((overflow === "auto" || overflow === "scroll") && node.scrollHeight > node.clientHeight) {
      return node;
    }
    node = node.parentElement;
  }
  return null;
}

/**
 * Attach to an input so focusing it brings it above the keyboard.
 *
 * ★ Used as a `ref`, so a screen writes `ref={keyboardAware}` and gets the
 * behaviour without wiring three listeners by hand each time -- which is how
 * this kind of thing ends up applied to four inputs out of six.
 */
export function keyboardAware(el: HTMLElement): void {
  const onFocus = () => keepVisible(el);
  el.addEventListener("focus", onFocus);
  onCleanup(() => el.removeEventListener("focus", onFocus));
}
