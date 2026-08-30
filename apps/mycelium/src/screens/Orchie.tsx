/**
 * ORCHIE — the phone-first curated face.
 *
 * Mycelium shows you a household. Orchie shows you **the few things that need
 * you**, one at a time, and nothing else. Same app, same engine, same gate, same
 * login — a different question.
 *
 * ★★★ **Nothing here ranks anything.** `compose(r)` in the engine binds widgets
 * to what happened, scores them `α·urgency + λ·relevance` under the household's
 * declared policy, and picks what fits the attention budget with a 0/1 knapsack.
 * This file renders what came back, in the order it came back.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * ★★★ **WHAT THIS REWRITE IS FOR.** The verdict on the first version was
 * "clunky and impossible to use, slow, and the keyboard isn't wired". All three
 * were true and none of them were the engine:
 *
 *   • It was drawn at the COCKPIT'S scale — 10-11px text, ~22px tap targets —
 *     on a phone, for people who are not looking for an excuse to squint.
 *   • Every tap waited on a round trip with nothing but a disabled button to
 *     show for it, and every refresh replaced the whole screen with the words
 *     "reading the household…", so the layout was thrown away and rebuilt.
 *   • Nothing knew the keyboard existed.
 *
 * So: Orchie's own scale (`ui/orchie.css.ts`), the keyboard wired for real
 * (`lib/viewport.ts`), and the classify flow promoted to the top of the screen
 * and opened on arrival — because THAT is the thing his mum actually does.
 *
 * ★★★ **The classifying job comes first, always.** Not because it ranks highest
 * — the engine decides ranking and this file does not argue — but because a
 * captured message is the one card that is a JOB rather than a READING.
 * Everything else on this screen is something to know; this is something to do.
 * ─────────────────────────────────────────────────────────────────────────────
 *
 * ★★★ **Every card can still say why it is here** — its real urgency, whether
 * that urgency was *measured*, its relevance, its score. And what stayed quiet
 * still says which kind of quiet: **withdrawn** (nothing to say, carries a
 * reason) or **outranked** (considered, carries a score).
 *
 * ★★ The capture flow asks **one question at a time**, with options that are the
 * household's own pocket names, and stops at a confirmation every time. A
 * history pre-fill saves the tap, never the confirm.
 */
import {
  createEffect,
  createResource,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
  untrack,
} from "solid-js";

import { onPulse } from "../lib/pulse";
import {
  engine,
  fmt,
  type FeedDto,
  type GateResult,
  type InferenceDto,
  type JsonValue,
  type SmsSweep,
  type CaptureContextDto,
  type ChoiceDto,
  type NettingDto,
  type OwnIdentifiersDto,
  type TransferDto,
  type DeviceDto,
  type SkipLearnedDto,
  type PersonHint,
} from "../lib/engine";
import { world } from "../lib/live";
import { keyboardAware, watchViewport } from "../lib/viewport";
import * as O from "../ui/orchie.css";

/** ★ The projection is a JSON value on the wire; read it by name, once. */
const read = (feed: FeedDto, key: string): JsonValue | undefined => {
  const r = feed.reading;
  return r && typeof r === "object" && !Array.isArray(r)
    ? (r as Record<string, JsonValue>)[key]
    : undefined;
};
const num = (feed: FeedDto, key: string): number => Number(read(feed, key) ?? 0);
const str = (feed: FeedDto, key: string): string => String(read(feed, key) ?? "—");

/** What each declared card is called, in words. ★ One place. */
const TITLE: Record<string, string> = {
  pocket_strain: "a pocket is under strain",
  classify_capture: "something needs classifying",
  household_summary: "the household",
  person_tab: "a running tab with someone",
  net_worth: "what the household is worth",
  recent_spend: "money went out",
  recent_income: "money came in",
};

/**
 * ★★ `onFace` is how a phone leaves Orchie.
 *
 * Optional, because Orchie must still render on its own; when the shell passes
 * it, the header grows the one control that makes the cockpit reachable from a
 * touch device. See `faceToggle` in orchie.css.ts for why it exists at all.
 */
/**
 * How many texts one call handles. Small enough that a page finishes fast, big
 * enough that a few thousand is a few dozen calls rather than a few thousand.
 */
const PAGE = 100;

/** How much of a message shows before it needs a tap. Most texts are shorter. */
const RAW_CLAMP = 220;

/**
 * How many pockets lead the picker.
 *
 * ★★ Four, the same number the attention budget uses. More choices at once is
 * a slower decision, not a better-informed one, and the rest are one tap away.
 */
// ★★ Eight rather than four, now that a pocket is a chip. The cap is about
//    how much there is to READ before choosing, and eight chips is about two
//    wrapped rows -- roughly the same amount of reading four full-width
//    buttons used to be, in a fraction of the height.
const SUGGESTIONS = 12;

/**
 * Is this a list to READ rather than a couple of buttons to hit?
 *
 * ★★★ Keyed on how many there are, never on which field it is. The previous
 * version asked whether the field was called `pocket_name`, which meant any
 * other long list silently got full-size buttons and — worse — a rename or a
 * new question with many answers would fail open, back to the wall of buttons
 * this exists to prevent. Length is the thing that actually decides it.
 *
 * ★★ Four is the line because that is the attention budget: at or under it a
 * person takes the options in at a glance and each deserves a proper target;
 * past it they are scanning, and scanning wants density.
 */
function manyToRead(opts: ChoiceDto[]): boolean {
  return opts.length > 4;
}

/** The pocket names in a state document, in the engine's own spelling. */
function pocketNames(state: unknown): string[] {
  const pockets = (state as { finances?: { pockets?: Record<string, unknown> } })?.finances?.pockets;
  return pockets ? Object.keys(pockets) : [];
}

/** How many captures are waiting, from the projection `compose(r)` ranked on. */
function waiting(feed: FeedDto): number {
  const n = Number(read(feed, "unclassified"));
  return Number.isFinite(n) ? n : 0;
}

/** A zeroed sweep, to add pages into. */
function blank(): SmsSweep {
  return {
    read: 0, applied: 0, needsYou: 0, duplicates: 0, unparsed: 0, refused: 0,
    skippedOtherSenders: 0, skippedSecrets: 0, failed: 0, firstFailure: null,
    hasMore: false, nextOffset: 0, remaining: 0, nettedPairs: 0,
  };
}

/** Adds one page into a running total, keeping the first failure reported. */
function add(total: SmsSweep, page: SmsSweep) {
  total.read += page.read;
  total.applied += page.applied;
  total.needsYou += page.needsYou;
  total.duplicates += page.duplicates;
  total.unparsed += page.unparsed;
  total.refused += page.refused;
  total.skippedOtherSenders += page.skippedOtherSenders;
  total.skippedSecrets += page.skippedSecrets;
  total.failed += page.failed;
  total.nettedPairs += page.nettedPairs;
  if (total.firstFailure === null) total.firstFailure = page.firstFailure;
}

/**
 * `+` for money in, `-` for money out.
 *
 * ★ Sustena's own two tokens: teal is what confirmed success already uses, and
 * amber is the default accent. No new colours, and none borrowed from anywhere
 * else's convention.
 */
function DirectionBadge(props: { direction: string | null }) {
  const inbound = () => props.direction === "received";
  return (
    <Show when={props.direction}>
      <span
        style={{
          color: inbound() ? "var(--teal, #4bb7a1)" : "var(--amber, #E8A020)",
          "font-weight": 600,
        }}
      >
        {inbound() ? "+" : "−"}
      </span>
    </Show>
  );
}

/** What a refusal for an under-funded pocket tells us, if that is what it is. */
type NoRoom = {
  pocket: string;
  remaining: number;
  requested: number;
  shortfall: number;
};

/**
 * Read the shortfall off a refusal.
 *
 * ★★ Only for `pocket_balance_sufficient`. Every other refusal keeps its plain
 * reason, because a branch that guessed at what to do about an unknown rule
 * would be worse than a sentence.
 */
function noRoom(v: GateResult): NoRoom | null {
  if (v.verdict === "admitted") return null;
  if (v.constraintViolated !== "pocket_balance_sufficient") return null;
  const d = v.data as Record<string, unknown> | null;
  if (!d || typeof d.pocket !== "string") return null;
  const n = (k: string) => (typeof d[k] === "number" ? (d[k] as number) : 0);
  const remaining = n("remaining");
  const requested = n("requested");
  return {
    pocket: d.pocket,
    remaining,
    requested,
    shortfall: n("shortfall"),
  };
}

/**
 * Say what is about to happen, in money and names.
 *
 * ★ The engine's own `why` names the operator and its arguments. True, and not
 * what a person needs while filing two thousand texts.
 */
function plainly(operator: string, params: unknown): string {
  const p = (params ?? {}) as Record<string, unknown>;
  const amount = typeof p.amount === "number" ? fmt(p.amount) : null;
  const pocket = typeof p.pocket_name === "string" ? p.pocket_name : null;
  if (operator === "budget.record_income" && amount) return `Looks like KES ${amount} received.`;
  if (amount && pocket) return `Looks like KES ${amount} to ${pocket}.`;
  if (amount) return `Looks like KES ${amount}.`;
  return "Ready to record.";
}

/**
 * The message, before the question about it.
 *
 * Amount and merchant first, because those are what a person files on. The raw
 * text underneath, because the parse is a reading of it and a reading can be
 * wrong. Everything here was already on the ingested message.
 */
function CaptureFacts(props: { head: CaptureContextDto }) {
  const [showRaw, setShowRaw] = createSignal(false);
  const money = () =>
    props.head.amount === null ? null : `KES ${fmt(props.head.amount)}`;
  return (
    <div class={O.stack}>
      <Show when={money()}>
        {(m) => (
          <p class={O.figure}>
            <DirectionBadge direction={props.head.direction} /> {m()}
          </p>
        )}
      </Show>
      <Show when={props.head.counterparty}>
        {(who) => <p class={O.cardTitle}>{who()}</p>}
      </Show>
      <p class={O.caption}>
        {props.head.source}
        <Show when={props.head.direction}>{(d) => <> · {d()}</>}</Show>
      </p>
      {/* ★★★ The text itself, because the amount and the merchant are a
          READING of it and a person filing two thousand of these needs to
          recognise the transaction, not just its summary. Short ones show
          whole; a long KCB message clamps so the buttons stay reachable, and
          opens on a tap. */}
      <Show
        when={props.head.raw.length > RAW_CLAMP}
        fallback={<p class={O.raw}>{props.head.raw}</p>}
      >
        <p class={O.raw}>
          {showRaw() ? props.head.raw : `${props.head.raw.slice(0, RAW_CLAMP)}…`}
        </p>
        <button class={O.linkish} onClick={() => setShowRaw((v) => !v)}>
          {showRaw() ? "▾ less" : "▸ show the whole message"}
        </button>
      </Show>
    </div>
  );
}

/**
 * Reading M-Pesa and KCB texts off this phone.
 *
 * Two things happen here. The button reads what is already in the inbox, which
 * is the point: nobody is going to paste a thousand messages by hand. And once
 * the permission is given, texts that arrive later are picked up on their own
 * and swept in the next time the app is open.
 *
 * The reason is shown before the system dialog, because a phone asking to read
 * your texts deserves an explanation first.
 */
function SmsCard(props: { sustainId: string; onSwept: () => void }) {
  const [perm, setPerm] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal<null | "asking" | "reading">(null);
  const [swept, setSwept] = createSignal<SmsSweep | null>(null);
  const [progress, setProgress] = createSignal(0);
  const [failed, setFailed] = createSignal<string | null>(null);

  onMount(() => {
    void engine
      .smsPermission()
      .then(setPerm)
      .catch(() => setPerm("unavailable"));
  });

  const ask = async () => {
    setBusy("asking");
    setFailed(null);
    try {
      setPerm(await engine.smsRequestPermission());
    } catch (e) {
      setFailed(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(null);
    }
  };

  /**
   * A page at a time, so the app keeps drawing.
   *
   * Reading everything in one call is what froze this button. The engine work
   * per message is small and there are thousands of them, so the total is long
   * and the interface had no way to say so. Each page is a separate call now,
   * the counts add up as they arrive, and the label says where it has got to.
   */
  const importInbox = async () => {
    setBusy("reading");
    setFailed(null);
    setProgress(0);
    const total = blank();
    try {
      let offset = 0;
      for (;;) {
        // 0 days means the whole inbox. Captures are deduped by the engine, so
        // running this again costs nothing.
        const page = await engine.smsImportPage(props.sustainId, 0, offset, PAGE);
        add(total, page);
        setProgress(total.read);
        setSwept({ ...total });
        if (!page.hasMore) break;
        offset = page.nextOffset;
        // Hand the frame back before the next page, so the count on screen is
        // one a person can actually watch move.
        await new Promise((r) => setTimeout(r, 0));
      }
      props.onSwept();
    } catch (e) {
      setFailed(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(null);
    }
  };

  // Not an Android build, so there is no inbox to read.
  return (
    <Show when={perm() !== "unavailable"}>
      <div class={O.card}>
        <p class={O.cardTitle}>your M-Pesa and KCB texts</p>

        <Show when={perm() === "granted"}>
          <p class={O.caption}>
            Orchie reads M-Pesa and KCB texts only. Money in is filed on its own.
            Money out waits for you to pick a pocket.
          </p>
          <button
            class={`${O.action.primary} ${O.actionWide}`}
            onClick={() => void importInbox()}
            disabled={busy() !== null}
          >
            <Show when={busy() === "reading"} fallback="read my texts">
              <span class={O.working} />{" "}
              {progress() === 0 ? "reading…" : `reading… ${progress()} so far`}
            </Show>
          </button>
        </Show>

        <Show when={perm() !== null && perm() !== "granted"}>
          <p class={O.caption}>
            To do this Orchie needs permission to read your texts. It reads M-Pesa
            and KCB only. Every other message on this phone is skipped on the
            phone itself and never leaves it, and anything carrying a one-time
            code is dropped without being stored.
          </p>
          <button
            class={`${O.action.primary} ${O.actionWide}`}
            onClick={() => void ask()}
            disabled={busy() !== null}
          >
            <Show when={busy() === "asking"} fallback="allow Orchie to read them">
              <span class={O.working} /> asking…
            </Show>
          </button>
          <Show when={perm() === "denied"}>
            <p class={O.caption}>
              Permission was refused. You can still paste a message by hand in
              Mycelium, under Ingest.
            </p>
          </Show>
        </Show>

        <Show when={swept()}>
          {(r) => (
            <div class={O.stack}>
              <p class={O.caption}>
                {r().read} text{r().read === 1 ? "" : "s"} read.{" "}
                {r().applied} filed, {r().needsYou} waiting for you,{" "}
                {r().duplicates} already seen.
              </p>
              {/* ★★ Each pair took TWO out of the queue and recorded nothing:
                  a charge and its refund are zero together. */}
              <Show when={r().nettedPairs > 0}>
                <p class={O.caption}>
                  {r().nettedPairs} refund{r().nettedPairs === 1 ? "" : "s"} cancelled against
                  {r().nettedPairs === 1 ? " its charge" : " their charges"}. Nothing recorded,
                  because together they are zero.
                </p>
              </Show>
              <Show when={r().unparsed > 0}>
                <p class={O.caption}>
                  {r().unparsed} in a shape no rule recognises yet.
                </p>
              </Show>
              <Show when={r().skippedOtherSenders + r().skippedSecrets > 0}>
                <p class={O.caption}>
                  {r().skippedOtherSenders} from other senders and{" "}
                  {r().skippedSecrets} carrying a code were skipped on the phone.
                </p>
              </Show>
              <Show when={r().failed > 0}>
                <p class={O.caption}>
                  {r().failed} could not be recorded. {r().firstFailure ?? ""}
                </p>
              </Show>
            </div>
          )}
        </Show>

        <Show when={failed()}>{(f) => <div class={O.errorBox}>{f()}</div>}</Show>
      </div>
    </Show>
  );
}

export default function Orchie(props: { onFace?: () => void }) {
  /**
   * ★★★ **`null`, never `""`. This one line was the whole bug.**
   *
   * `createResource` skips its fetcher only for `false | null | undefined`. An
   * empty string is a perfectly ordinary value, so `?? ""` did not mean "no
   * selection yet" to Solid -- it meant "go and fetch the Sustain whose id is
   * the empty string". The engine answered exactly as it should:
   *
   *     Error: no Sustain called ''
   *
   * and the resource wedged on that first rejection. On the phone the screen
   * then sat on its loading skeleton for ever, on real data, with no error
   * shown, because the failed fetch was for an id nobody had asked about.
   *
   * ★★ It only ever bit on a phone, which is why it survived every check. The
   * unlocked tree renders the moment the identity opens, while `refreshWorld`
   * is still in flight -- so there is a real window where nothing is selected
   * yet. On a laptop the face is Mycelium and Orchie is not mounted during that
   * window; on a phone Orchie IS the face, so it mounts straight into it, every
   * single time.
   */
  const sustain = () => world.selected;
  const [feed, { refetch }] = createResource(sustain, (id) => engine.feed(id, null));

  /**
   * ★★★ **Orchie's liveness, and it is one line because of how Orchie is
   *     built — Multiparty §III.**
   *
   * Every card on this shell reads the SAME `feed`: the queue, accounts,
   * inventory, filed spends, own numbers, net worth, tabs, the SMS card and
   * the classifier. One subscription makes all of them live, and there is no
   * per-card wiring to forget.
   *
   * Before this, `feed` refetched only when the PERSON did something --
   * `onChanged` after classifying, sweeping, filing. That covers what he
   * causes and nothing else: a change made on the laptop, or state arriving
   * from a peer, reached the store and this shell went on showing what it had.
   * §III is the relay for exactly that, and `onPulse`'s refractory window is
   * what stops a burst of arriving entries queueing one refetch each.
   */
  onPulse(refetch);

  /**
   * Texts that arrived while the app was shut are sitting in a local queue,
   * because the receiver cannot write to the engine: a write needs the
   * unlocked key and a text usually lands while the phone is locked. This is
   * the other half. It runs when the household opens and again whenever the
   * app comes back to the front, and it says nothing unless it found
   * something.
   */
  const drain = async () => {
    const id = sustain();
    if (!id) return;
    try {
      let handled = 0;
      for (;;) {
        const batch = await engine.smsDrain(id, PAGE);
        handled += batch.read;
        if (!batch.hasMore) break;
        // Give the frame back between batches. Draining the whole queue in one
        // call is what made unlocking freeze when texts had piled up.
        await new Promise((r) => setTimeout(r, 0));
      }
      // ★★★ Cancel refunds against their charges, once per open.
      //
      // The backlog was captured before netting existed, so without this his
      // existing refunds would sit in the queue forever waiting for an import
      // that never comes. Idempotent: a second pass finds nothing left to do.
      let moved = 0;
      try {
        const n = await engine.netReversals(id);
        moved = n.netted + n.givenBack;
        if (n.netted > 0 || n.givenBack > 0 || n.uncompensable > 0) setNetting(n);
      } catch {
        // Nothing to say. The queue is unchanged.
      }
      // ★★ And his own money moving, for the same reason netting runs here:
      //    the texts were captured before he had said which numbers are his,
      //    so without this pass they would wait for an import that never comes.
      try {
        const t = await engine.applyTransfers(id);
        if (t.moved > 0) {
          moved += t.moved;
          setMoves(t);
        }
      } catch {
        // No numbers declared yet, which is the ordinary first run.
      }
      if (handled > 0 || moved > 0) await refetch();

      // ★★★ The prompt comes down only once the queue has actually been
      //     swept. Cancelling it on launch would say the work is done while
      //     the texts were still sitting there, and a prompt that lies once is
      //     one that gets swiped away every time after.
      try {
        await engine.smsClearPrompt();
      } catch {
        // Not an Android build, or notifications were never granted.
      }
    } catch {
      // No permission yet, or not an Android build. Nothing to say.
    }
  };

  /**
   * A launch that came from tapping the notification.
   *
   * ★★★ The tap is answered by the ORDERING rather than by carrying a target
   * into the view: the notification always fires for the text that just
   * arrived, and the queue is newest-first, so the card already waiting is the
   * one he tapped about. Threading an explicit id through `compose(r)` would
   * add a second way to choose what is on screen, and two answers to "what am
   * I looking at" is how a surface starts disagreeing with itself.
   *
   * What consuming the target IS for: knowing the launch was a tap at all, so
   * the sweep runs immediately instead of waiting for the next visibility
   * change — and taking the target once, so it cannot re-fire on an unrelated
   * launch later.
   */
  const answerTap = async () => {
    try {
      const pending = await engine.smsPendingClassify();
      if (pending) await drain();
    } catch {
      // Not an Android build. There is no tap to answer.
    }
  };
  createEffect(() => {
    if (sustain()) void drain();
  });
  onMount(() => {
    void answerTap();
    const onVisible = () => {
      if (document.visibilityState === "visible") {
        void drain();
        // ★★ A tap can land on an app that was merely backgrounded, which
        //    never remounts. Asked here too, for the same reason the drain is.
        void answerTap();
      }
    };
    document.addEventListener("visibilitychange", onVisible);
    onCleanup(() => document.removeEventListener("visibilitychange", onVisible));
  });
  const [showQuiet, setShowQuiet] = createSignal(false);
  /**
   * What the last netting pass did, so a count that fell says why.
   *
   * Cancelling and giving back are different events and read differently: one
   * removed two questions and touched no money, the other put money back.
   */
  const [netting, setNetting] = createSignal<NettingDto | null>(null);
  /** His own moves recognised on this open, so a count never drops unexplained. */
  const [moves, setMoves] = createSignal<TransferDto | null>(null);

  onMount(watchViewport);

  /**
   * ★★ Has the first load been going on unreasonably long?
   *
   * Only ever used to add a line of explanation to the skeleton. It changes
   * nothing about what is fetched and it never hides anything -- it exists
   * because a loading state with no upper bound is indistinguishable from a
   * broken one, and that is exactly how the empty-string stall stayed hidden.
   */
  const [slow, setSlow] = createSignal(false);
  onMount(() => {
    const t = window.setTimeout(() => setSlow(true), 10_000);
    onCleanup(() => window.clearTimeout(t));
  });

  // ★★★ `feed.latest` rather than `feed()`. During a refetch, `feed()` is
  //     undefined and the whole screen would fall back to a placeholder — which
  //     is what made every confirmation feel like a stall. `latest` holds the
  //     previous answer until the new one lands, so the layout never collapses;
  //     it dims (see `staleWhileRefreshing`) and the numbers change in place.
  const shown = () => feed.latest;

  /**
   * The standing rows. ★ Bounded by what they are, rather than by how many
   * events happened: at most one strained pocket. Captures are NOT here; they
   * are the `classify_capture` widget, which `compose(r)` scores and the
   * knapsack bounds like every other card.
   */
  const notices = () => shown()?.attention ?? [];

  return (
    <div class={O.frame} data-face="orchie">
      <div class={O.column}>
        <div class={O.header}>
          <span class={O.brand}>ORCHIE</span>
          <span class={O.headerMeta}>{shown()?.label ?? ""}</span>
          {/* ★★ The cockpit, which is where Ingest lives -- the source picker
              and the paste field for a real M-Pesa or KCB message. Reachable
              from the phone the messages arrive on, which it was not. */}
          <Show when={props.onFace}>
            {(go) => (
              <button class={O.faceToggle} onClick={() => go()()}>
                mycelium
              </button>
            )}
          </Show>
        </div>

        {/* ★★★ **Every way of having nothing to show now says which one it is.**
            The stall that shipped was invisible precisely because "loading" and
            "wedged" looked identical -- an endless shimmer with no way to tell
            them apart, on either side of the screen. Four distinct answers now,
            and the last one exists so that a stall can never be silent again. */}

        {/* the world itself could not be read */}
        <Show when={world.error && !shown()}>
          <div class={O.errorBox}>
            the household could not be opened: {world.error}
          </div>
        </Show>

        {/* the world opened, and there is genuinely nothing in it */}
        <Show when={!world.error && world.loaded && !world.selected}>
          <p class={O.empty}>
            no household on this device yet
            <br />
            nothing to show until there is one
          </p>
        </Show>

        {/* the feed itself refused or failed */}
        <Show when={feed.error && !shown()}>
          <div class={O.errorBox}>
            the household could not be read: {String(feed.error)}
          </div>
        </Show>

        {/* genuinely still loading */}
        <Show when={!shown() && !feed.error && !world.error && !(world.loaded && !world.selected)}>
          <FeedSkeleton />
          {/* ★★ A shimmer that never ends is a lie of omission. After ten
              seconds this stops pretending it is nearly there. */}
          <Show when={slow()}>
            <p class={O.caption}>
              this is taking longer than usual
            </p>
          </Show>
        </Show>

        <Show when={shown()}>
          {(f) => (
            <div
              class={O.stack}
              classList={{ [O.staleWhileRefreshing]: feed.loading }}
            >
              {/* ═══ THE JOB — one message, whatever the queue holds ══════
                  This used to be a `For` over a row per waiting capture, which
                  on a real inbox meant thousands of open classify flows and a
                  frozen screen. One at a time now: the feed hands over a single
                  id, and the next arrives when this one is done. */}
              {/* ★★★ `keyed`, and it is the whole bug.
                  `Show` without it KEEPS the same children when `when` goes
                  from one truthy value to another. So filing a message left
                  the previous card's instance in place for the next one: its
                  `onMount` had already run, its step had been reset, and it
                  rendered the text with no actions at all. Keyed on the
                  message id, every message gets its own card, armed the same
                  way, by construction rather than by remembering to re-arm. */}
              {/* ★★ Keyed on the ID, not the object. The feed hands back a
                  fresh object on every read, so keying on it would re-create
                  the card whenever anything refetched and throw away answers
                  given halfway through. The id changes exactly when the
                  subject does, which is exactly when a new card is right. */}
              <QueueCard feed={f()} onChanged={() => void refetch()} />

              {/* ═══ the calm read ═══════════════════════════════════════ */}
              <Summary feed={f()} />

              {/* ═══ notices that are not a classifiable capture ═════════ */}
              <Show when={notices().length > 0}>
                <div class={O.card}>
                  <h2 class={O.cardTitle}>needs you</h2>
                  <For each={notices()}>
                    {(a) => (
                      <div class={O.stack}>
                        <div class={O.row}>
                          <span class={O.badge[a.severity === "danger" ? "danger" : "warn"]}>
                            {a.severity === "danger" ? "urgent" : "soon"}
                          </span>
                          <span class={O.body}>{a.what}</span>
                        </div>
                        <p class={O.caption}>{a.why}</p>
                      </div>
                    )}
                  </For>
                </div>
              </Show>

              {/* ═══ the ranked feed ═════════════════════════════════════ */}
              <Show
                when={f().cards.length > 0}
                fallback={
                  <Show when={!f().queueHead}>
                    <p class={O.empty}>nothing needs you right now</p>
                  </Show>
                }
              >
                <For each={f().cards}>
                  {(c) => (
                    <div class={O.card}>
                      <div class={O.cardHead}>
                        <h2 class={O.cardTitle}>{TITLE[c.id] ?? c.id}</h2>
                        <span class={O.spacer} />
                        <Show when={c.urgency > 0}>
                          <span class={O.badge[c.urgency > 0.5 ? "danger" : "warn"]}>
                            {Math.round(c.urgency * 100)}%
                          </span>
                        </Show>
                      </div>
                      <CardBody card={c} feed={f()} />
                      <Why card={c} />
                    </div>
                  )}
                </For>
              </Show>

              {/* ★★ A count that fell on its own needs a reason on screen. */}
              <Show when={netting()}>
                {(n) => (
                  <div class={O.card}>
                    <Show when={n().netted > 0}>
                      <p class={O.caption}>
                        {n().netted} refund{n().netted === 1 ? "" : "s"} cancelled against
                        {n().netted === 1 ? " its charge" : " their charges"}. Both left the
                        list and nothing was recorded, because together they are zero.
                      </p>
                    </Show>
                    {/* ★★★ This one really moved money, so it says so plainly. */}
                    <Show when={n().givenBack > 0}>
                      <p class={O.caption}>
                        {n().givenBack} refund{n().givenBack === 1 ? "" : "s"} of money you had
                        already filed went back to where {n().givenBack === 1 ? "it" : "they"} came
                        from. Your pockets and balance are where they were before the charge.
                      </p>
                    </Show>
                    <Show when={n().refused > 0}>
                      <p class={O.caption}>
                        {n().refused} could not go back yet and {n().refused === 1 ? "is" : "are"}{" "}
                        still in the list.
                      </p>
                    </Show>
                    <Show when={n().uncompensable > 0}>
                      <p class={O.caption}>
                        {n().uncompensable} refund{n().uncompensable === 1 ? "" : "s"} matched
                        something you filed before Orchie kept track of which pocket it went to,
                        so {n().uncompensable === 1 ? "it is" : "they are"} still in the list for
                        you to place.
                      </p>
                    </Show>
                    <button class={O.linkish} onClick={() => setNetting(null)}>
                      got it
                    </button>
                  </div>
                )}
              </Show>

              <Show when={f().device}>{(d) => <DeviceCard device={d()} />}</Show>
              <AccountsCard feed={f()} onChanged={() => void refetch()} />
              <InventoryCard feed={f()} onChanged={() => void refetch()} />
              <FiledCard feed={f()} onChanged={() => void refetch()} />
              <OwnNumbersCard sustainId={f().sustainId} onChanged={() => void refetch()} />

              <Show when={moves()}>
                {(t) => (
                  <div class={O.card}>
                    <p class={O.caption}>
                      {t().moved} of your own {t().moved === 1 ? "move" : "moves"} between your
                      accounts {t().moved === 1 ? "was" : "were"} recognised and recorded as money
                      changing place. Neither spending nor income.
                    </p>
                    <button class={O.linkish} onClick={() => setMoves(null)}>
                      got it
                    </button>
                  </div>
                )}
              </Show>

              {/* ═══ read the phone's own texts ══════════════════════════ */}
              <SmsCard sustainId={f().sustainId} onSwept={() => void refetch()} />

              {/* ═══ narrate anything ════════════════════════════════════ */}
              <div class={O.card}>
                <h2 class={O.cardTitle}>something else happened?</h2>
                <Classify sustain={f().sustainId} messageId={null} onDone={() => void refetch()} />
              </div>

              {/* ═══ what stayed quiet ═══════════════════════════════════ */}
              <Show when={f().quiet.length > 0}>
                <button class={O.linkish} onClick={() => setShowQuiet((q) => !q)}>
                  {showQuiet() ? "▾" : "▸"} {f().quiet.length} other{" "}
                  {f().quiet.length === 1 ? "thing" : "things"} stayed quiet
                </button>
                <Show when={showQuiet()}>
                  <div class={O.card}>
                    <For each={f().quiet}>
                      {(q) => (
                        <div class={O.stack}>
                          <div class={O.row}>
                            <span class={O.body}>{TITLE[q.id] ?? q.id}</span>
                            <span class={O.spacer} />
                            {/* ★★ Withdrawn and outranked are different facts. */}
                            <span class={O.badge.quiet}>
                              {q.withdrew ? "nothing to say" : "not now"}
                            </span>
                          </div>
                          {/* ★ The engine's reason names the dimensions that
                              did not resolve, which is exactly right in the
                              cockpit and noise on a phone. The badge above
                              already says the useful half. */}
                        </div>
                      )}
                    </For>
                    {/* ★ The engine's own bookkeeping is real and belongs in
                        Mycelium, not on a phone. What a person needs here is
                        that these were considered and did not make the cut. */}
                    <p class={O.caption}>
                      showing {f().spent} of {f().budget}
                    </p>
                  </div>
                </Show>
              </Show>
            </div>
          )}
        </Show>
      </div>
    </div>
  );
}

/**
 * ★★★ The first-load placeholder, in the SHAPE of the answer.
 *
 * Not a spinner. A spinner says "wait" and nothing else; a skeleton says "a
 * heading, then a number, then two cards" — so the eye is already where the
 * content will be, and the arrival is a fill rather than a jump.
 */
function FeedSkeleton() {
  return (
    <div class={O.stack}>
      <div class={O.card}>
        <div class={O.skelLine.title} />
        <div class={O.skelLine.figure} />
        <div class={O.skelLine.short} />
      </div>
      <div class={O.card}>
        <div class={O.skelLine.title} />
        <div class={O.skelLine.text} />
        <div class={O.skelLine.short} />
      </div>
    </div>
  );
}

/** The calm read: one plain figure, never the machinery. */
/**
 * The phone, watching itself being watched.
 *
 * ★★★ Only ever appears when there is something to say. A card reporting that
 * everything is fine, every time, is a card the eye learns to skip — and then
 * it is skipped on the day it says something else. Silence is the healthy
 * state; this speaks when the phone has gone quiet or fallen behind.
 */
function DeviceCard(props: { device: DeviceDto }) {
  const quiet = () => props.device.quietForMinutes;
  const howLong = () => {
    const m = quiet();
    if (m === null || m === undefined) return "since it was set up";
    if (m < 90) return `for ${m} minutes`;
    const h = Math.round(m / 60);
    if (h < 48) return `for ${h} hours`;
    return `for ${Math.round(h / 24)} days`;
  };

  return (
    <Show when={props.device.stale || props.device.queueDepth > 0}>
      <div class={O.card}>
        <h2 class={O.cardTitle}>this phone</h2>
        <Show when={props.device.stale}>
          <p class={O.caption}>
            Orchie has not caught a text {howLong()}. If that is not right, open the app and read
            your texts to catch up.
          </p>
        </Show>
        {/* ★★ The leading indicator. A phone that cannot deliver keeps
            accepting, so this rises before anything else looks wrong. */}
        <Show when={props.device.queueDepth > 0}>
          <p class={O.caption}>
            {props.device.queueDepth} text{props.device.queueDepth === 1 ? "" : "s"} caught and
            waiting to be handed over.
          </p>
        </Show>
      </div>
    </Show>
  );
}

/**
 * One message at a time, and a way to walk between them.
 *
 * ★★★ The card used to show whatever was oldest and nothing else. There was no
 * way back to something just filed and no way past something he could not
 * answer — so a message he did not understand blocked the whole queue, and a
 * filing he got wrong was unreachable the moment it landed.
 *
 * ★★ Still ONE card. The queue is a list to STEP THROUGH, never a list to
 * render: showing them all is the flood the attention budget exists to
 * prevent, and it froze the app once already.
 */
function QueueCard(props: { feed: FeedDto; onChanged: () => void }) {
  const queue = () => props.feed.queue;
  // ★★ The index is held here and only reset when the queue's own identity
  //    changes, so a background refetch does not throw away where he is.
  const [at, setAt] = createSignal<number | null>(null);
  const idx = () => {
    const i = at();
    const max = Math.max(0, queue().length - 1);
    if (i === null) return Math.min(props.feed.queueStart, max);
    return Math.min(Math.max(0, i), max);
  };
  const current = () => queue()[idx()];
  const [busy, setBusy] = createSignal(false);

  const back = () => setAt(Math.max(0, idx() - 1));

  /**
   * Forward IS the honest defer.
   *
   * ★★★ Not a separate button, because they are one act: "I cannot answer this
   * yet, show me the next one." Deferring puts it at the TOP next time he
   * opens the app — otherwise putting something off would be
   * indistinguishable from throwing it away.
   *
   * ★★ Already-answered messages just advance. There is nothing to defer about
   * a decision he has made.
   */
  const forward = async () => {
    const c = current();
    const next = Math.min(queue().length - 1, idx() + 1);
    if (c && c.status !== "processed") {
      setBusy(true);
      try {
        await engine.deferMessage(props.feed.sustainId, c.id);
      } catch {
        // Nothing to say: the card moves on either way, and the message is
        // still in the queue.
      } finally {
        setBusy(false);
      }
    }
    setAt(next);
  };

  const label = () => {
    const c = current();
    if (!c) return "";
    if (c.status === "processed") return "processed";
    if (c.status === "deferred") return "you put this off";
    return "";
  };

  return (
    <Show when={queue().length > 0}>
      <div class={O.cardPrimary}>
        <div class={O.row}>
          {/* ★★ Back is plain navigation and never changes anything. */}
          <button
            class={O.linkish}
            disabled={idx() === 0}
            onClick={back}
            aria-label="previous message"
          >
            ‹ back
          </button>
          <span class={O.spacer} />
          <span class={O.caption}>
            {idx() + 1} of {queue().length}
            <Show when={label()}> · {label()}</Show>
          </span>
          <span class={O.spacer} />
          <button
            class={O.linkish}
            disabled={busy() || idx() >= queue().length - 1}
            onClick={() => void forward()}
            aria-label="skip for later and go to the next"
          >
            {busy() ? "…" : "later ›"}
          </button>
        </div>

        <Show when={current()} keyed>
          {(c) => (
            <>
              {/* ★★★ A processed message stays fully editable. Saying
                  "processed" and then refusing to change it would be a status
                  that punishes him for having answered. */}
              <Show when={c.status === "processed"}>
                <p class={O.caption}>
                  Filed
                  <Show when={c.filedPocket}>{(p) => <> to {p()}</>}</Show>
                  <Show when={c.filedAmount}>{(a) => <> · {fmt(a())}</>}</Show>. You can change it
                  below — the first filing stays on the record and the change is added after it.
                </p>
              </Show>
              <CaptureFacts head={c} />
              <Classify
                sustain={props.feed.sustainId}
                messageId={c.id}
                autoStart={c.status !== "processed"}
                personPockets={props.feed.personPockets}
                backfill
                onDone={props.onChanged}
              />
            </>
          )}
        </Show>
      </div>
    </Show>
  );
}

/**
 * "This number is somebody's tab."
 *
 * ★★★ A pocket tied to a real number stops being an envelope and becomes a
 * running account with a person: money sent to them comes out of it, money
 * received from them goes back into it, and the two net. Without the link the
 * same person's money arrives as unrelated household income while their tab
 * still shows everything outstanding — both halves wrong, and nothing saying so.
 *
 * ★★ Offered only where it makes sense: a message that actually printed a
 * number, and only after something was really filed to a pocket. It never
 * appears for a number already linked, because there is nothing to ask.
 *
 * ★★★ The number has to be typed in full, and the card says why rather than
 * silently rejecting a paste of what is on screen. Messages print it masked;
 * a mask is missing its middle, so linking one would claim an identity nobody
 * actually gave and every later match would inherit the guess.
 */
function LinkNumberControl(props: { sustain: string; messageId: string; pocket: string }) {
  const [hint, setHint] = createSignal<PersonHint | null>(null);
  const [open, setOpen] = createSignal(false);
  const [typed, setTyped] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [done, setDone] = createSignal(false);
  const [failed, setFailed] = createSignal<string | null>(null);

  createEffect(() => {
    const id = props.messageId;
    void engine
      .personHint(props.sustain, id)
      .then(setHint)
      .catch(() => setHint(null));
  });

  const link = async () => {
    setBusy(true);
    setFailed(null);
    try {
      const r = await engine.linkNumber(props.sustain, props.pocket, typed().trim());
      if (r.verdict === "admitted") {
        setDone(true);
      } else {
        setFailed(r.reason ?? "the engine refused it");
      }
    } catch (e) {
      setFailed(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Show when={hint()?.printed && !hint()?.pocket}>
      {(_) => (
        <Show
          when={!done()}
          fallback={
            <p class={O.caption}>
              linked · money to and from this number is {props.pocket}'s tab from now on
            </p>
          }
        >
          <Show
            when={open()}
            fallback={
              <button
                class={`${O.action.secondary} ${O.actionWide}`}
                onClick={() => setOpen(true)}
              >
                this number is {props.pocket}'s tab
              </button>
            }
          >
            <div class={O.stack}>
              <p class={O.caption}>
                The message only shows {hint()?.printed}. Type the whole number, so it can be
                matched with certainty rather than guessed at.
              </p>
              <input
                class={O.input}
                type="tel"
                inputmode="tel"
                placeholder="0712345678"
                value={typed()}
                onInput={(e) => setTyped(e.currentTarget.value)}
              />
              <Show when={failed()}>{(f) => <div class={O.errorBox}>{f()}</div>}</Show>
              <button
                class={`${O.action.primary} ${O.actionWide}`}
                disabled={busy() || typed().trim().length < 9}
                onClick={() => void link()}
              >
                {busy() ? "linking…" : `link to ${props.pocket}`}
              </button>
              <button class={O.linkish} onClick={() => setOpen(false)}>
                not now
              </button>
            </div>
          </Show>
        </Show>
      )}
    </Show>
  );
}

/**
 * Spends already filed, so one filed to the wrong pocket can be reached.
 *
 * ★★★ This surface did not exist. A tx could be classified and then never
 * touched again — there was no list of recorded spends anywhere in Orchie, so
 * a wrong pocket was permanent no matter what the engine could do about it. A
 * correction operator with nothing to correct FROM is not a correction path.
 *
 * ★★ Collapsed by default and short. Fixing last week's shopping is a real
 * need; scrolling a year of it is a different screen nobody has asked for.
 */
function FiledCard(props: { feed: FeedDto; onChanged: () => void }) {
  const [open, setOpen] = createSignal(false);
  const [moving, setMoving] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [failed, setFailed] = createSignal<string | null>(null);
  const [done, setDone] = createSignal<string | null>(null);

  const pockets = () => props.feed.inventory.map((g) => g.pocket);
  /** Every pocket the household has, not only the ones holding something. */
  const allPockets = () => {
    const seen = new Set<string>([...pockets(), ...props.feed.filed.map((f) => f.pocket)]);
    return [...seen].sort();
  };

  const move = async (f: { messageId: string; pocket: string; amount: number }, to: string) => {
    setBusy(true);
    setFailed(null);
    try {
      const r = await engine.reclassify(props.feed.sustainId, f.messageId, f.pocket, to, f.amount);
      if (r.verdict !== "admitted") {
        // ★ The engine's own words. A pocket with no room says so here in the
        //   same terms it does when a spend is first filed.
        setFailed(r.reason ?? "the engine refused it");
        return;
      }
      setDone(`moved to ${to}`);
      setMoving(null);
      props.onChanged();
    } catch (e) {
      setFailed(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Show when={props.feed.filed.length > 0}>
      <div class={O.card}>
        <button class={O.linkish} onClick={() => setOpen((o) => !o)}>
          {open() ? "▾" : "▸"} filed recently ({props.feed.filed.length})
        </button>
        <Show when={open()}>
          <For each={props.feed.filed}>
            {(f) => (
              <>
                <div class={O.row}>
                  <span class={O.caption}>
                    {f.counterparty || "a payment"} · {fmt(f.amount)} · {f.pocket}
                  </span>
                  <span class={O.spacer} />
                  <button
                    class={O.linkish}
                    onClick={() => setMoving((m) => (m === f.messageId ? null : f.messageId))}
                  >
                    change pocket
                  </button>
                </div>
                <Show when={moving() === f.messageId}>
                  <div class={O.chips}>
                    <For each={allPockets().filter((p) => p !== f.pocket)}>
                      {(p) => (
                        <button class={O.chip} disabled={busy()} onClick={() => void move(f, p)}>
                          {p}
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </>
            )}
          </For>
          <Show when={failed()}>{(e) => <div class={O.errorBox}>{e()}</div>}</Show>
          <Show when={done()}>
            {(d) => (
              <p class={O.caption}>
                {d()}. The original filing stays on the record — this was added after it, not
                instead of it.
              </p>
            )}
          </Show>
        </Show>
      </div>
    </Show>
  );
}

/**
 * What the household holds.
 *
 * ★★★ The other half of every spend, and the half nothing recorded until now.
 * A pocket says what left; this says what came back the other way. Grouped by
 * the pocket that bought it, because the question people actually ask is "what
 * did the food money buy", not "what do I own".
 *
 * ★★ Silent until there is something in it. A card that says "nothing yet" on
 * every open is a card the eye learns to skip.
 */
function InventoryCard(props: { feed: FeedDto; onChanged: () => void }) {
  const [open, setOpen] = createSignal<string | null>(null);
  const [using, setUsing] = createSignal<string | null>(null);
  const [failed, setFailed] = createSignal<string | null>(null);

  /**
   * ★★★ The real expense.
   *
   * Whole-asset by default, because "used up" is what someone says when a
   * thing is finished. Using part of one is a real case the operator handles
   * and this does not ask for yet — the amount field is the next slice, and
   * offering a form here before the loop is proven would be guessing at how
   * he wants to say it.
   */
  const useUp = async (a: { id: string; item: string }) => {
    setUsing(a.id);
    setFailed(null);
    try {
      const r = await engine.confirm(
        props.feed.sustainId,
        "inventory.consume",
        { asset_id: a.id },
        null,
        null,
      );
      if (r.verdict !== "admitted") {
        setFailed(r.reason ?? "the engine refused it");
        return;
      }
      props.onChanged();
    } catch (e) {
      setFailed(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setUsing(null);
    }
  };
  const groups = () => props.feed.inventory;
  const held = () =>
    Math.round(groups().reduce((t, g) => t + g.total, 0) * 100) / 100;

  return (
    <Show when={groups().length > 0}>
      <div class={O.card}>
        <span class={O.figureLabel}>what you hold</span>
        <span class={O.figure}>{fmt(held())}</span>
        <For each={groups()}>
          {(g) => (
            <>
              <button
                class={O.linkish}
                onClick={() => setOpen((o) => (o === g.pocket ? null : g.pocket))}
              >
                {open() === g.pocket ? "▾" : "▸"} {g.pocket} · {fmt(g.total)} ·{" "}
                {g.assets.length} {g.assets.length === 1 ? "thing" : "things"}
              </button>
              <Show when={open() === g.pocket}>
                <For each={g.assets}>
                  {(a) => (
                    <div class={O.row}>
                      <span class={O.caption}>
                        {a.item}
                        <Show when={a.subpocket}>{(sp) => <> · {sp()}</>}</Show>
                      </span>
                      <span class={O.spacer} />
                      <span class={O.caption}>{fmt(a.value)}</span>
                      {/* ★★★ Where the money is really spent. Buying it left
                          the household no poorer; using it up is what does. */}
                      <button
                        class={O.linkish}
                        disabled={using() === a.id}
                        onClick={() => void useUp(a)}
                      >
                        {using() === a.id ? "…" : "used up"}
                      </button>
                    </div>
                  )}
                </For>
              </Show>
            </>
          )}
        </For>
        <Show when={failed()}>{(e) => <div class={O.errorBox}>{e()}</div>}</Show>
      </div>
    </Show>
  );
}

/**
 * The numbers he calls his own.
 *
 * ★★★ Moving money from KCB to M-Pesa produces two texts that each read like
 * an ordinary transaction. Nothing in the wording separates that from paying a
 * friend the same amount; the only thing that does is whether the number on the
 * other side is his. So it has to be asked, and it is asked here, on the phone,
 * and stored on the phone. Nothing sends it anywhere.
 */
function OwnNumbersCard(props: { sustainId: string; onChanged: () => void }) {
  const [own, setOwn] = createSignal<OwnIdentifiersDto | null>(null);
  const [open, setOpen] = createSignal(false);
  const [mpesa, setMpesa] = createSignal("");
  const [kcb, setKcb] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [failed, setFailed] = createSignal<string | null>(null);
  const [found, setFound] = createSignal<TransferDto | null>(null);

  const load = async () => {
    try {
      const o = await engine.ownIdentifiers();
      setOwn(o);
      setMpesa(o.mpesa.join(", "));
      setKcb(o.kcb.join(", "));
    } catch {
      // Nothing declared yet is the ordinary case, not an error.
    }
  };
  void load();

  const split = (v: string) =>
    v
      .split(",")
      .map((x) => x.trim())
      .filter((x) => x !== "");

  const save = async () => {
    setSaving(true);
    setFailed(null);
    try {
      const o = await engine.setOwnIdentifiers({ mpesa: split(mpesa()), kcb: split(kcb()) });
      setOwn(o);
      setOpen(false);
      // ★★ Now that it knows which numbers are his, look for his own moves --
      //    and say what it found. Doing this silently would leave a question
      //    disappearing from the list with no explanation.
      try {
        const t = await engine.applyTransfers(props.sustainId);
        setFound(t);
        if (t.moved > 0) props.onChanged();
      } catch {
        // Nothing found is the ordinary case on a first run.
      }
    } catch (e) {
      setFailed(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setSaving(false);
    }
  };

  const declared = () => (own()?.mpesa.length ?? 0) + (own()?.kcb.length ?? 0);

  return (
    <div class={O.card}>
      <h2 class={O.cardTitle}>your own numbers</h2>
      <Show
        when={open()}
        fallback={
          <>
            <p class={O.caption}>
              {declared() === 0
                ? "Moving money between your own accounts looks exactly like paying someone else. Tell Orchie your numbers and it can tell the difference."
                : `${declared()} number${declared() === 1 ? "" : "s"} saved on this phone.`}
            </p>
            <button class={O.linkish} onClick={() => setOpen(true)}>
              {declared() === 0 ? "add my numbers" : "change them"}
            </button>
          </>
        }
      >
        <p class={O.caption}>
          Your M-Pesa number, and your KCB account number. They stay on this phone.
        </p>
        <input
          class={O.input}
          value={mpesa()}
          placeholder="my M-Pesa number"
          inputmode="tel"
          onInput={(e) => setMpesa(e.currentTarget.value)}
        />
        <input
          class={O.input}
          value={kcb()}
          placeholder="my KCB account number"
          inputmode="numeric"
          onInput={(e) => setKcb(e.currentTarget.value)}
        />
        <div class={O.row}>
          <button class={O.linkish} disabled={saving()} onClick={() => void save()}>
            {saving() ? "saving…" : "save"}
          </button>
          <button class={O.linkish} onClick={() => setOpen(false)}>
            cancel
          </button>
        </div>
        <Show when={failed()}>
          <p class={O.caption}>{failed()}</p>
        </Show>
      </Show>

      {/* ★★★ A question that vanished from the list needs a reason on screen. */}
      <Show when={found()}>
        {(t) => (
          <>
            <Show when={t().moved > 0}>
              <p class={O.caption}>
                {t().moved} of your own {t().moved === 1 ? "move" : "moves"} between accounts
                recognised. {t().moved === 1 ? "It is" : "They are"} recorded as money changing
                place, not as spending or income.
              </p>
            </Show>
            {/* ★★★ A queue that GREW needs a reason as much as one that
                shrank. He set these aside; something overturned that, and he
                should hear it from the app rather than find them back. */}
            <Show when={t().reclaimed > 0}>
              <p class={O.caption}>
                {t().reclaimed} you had skipped turned out to be{" "}
                {t().reclaimed === 1 ? "half of a real move" : "halves of real moves"} between
                your accounts — the reference codes matched, so{" "}
                {t().reclaimed === 1 ? "it is" : "they are"} back.
              </p>
            </Show>
            <Show when={t().unpaired > 0}>
              <p class={O.caption}>
                {t().unpaired} looks like your own money moving but only one side of it is here,
                so {t().unpaired === 1 ? "it stays" : "they stay"} in the list.
              </p>
            </Show>
            <Show when={t().blocked > 0}>
              <p class={O.caption}>
                {t().blocked} could not be undone from what was already filed.
              </p>
            </Show>
            <Show when={t().moved === 0 && t().unpaired === 0 && t().blocked === 0}>
              <p class={O.caption}>Nothing in the list looks like your own money moving.</p>
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}

/**
 * Where the money is, as against what it is for.
 *
 * A pocket answers "what is this for". An account answers "where is it". They
 * are two readings of the same shilling, and this card is the second one.
 *
 * ★★★ The gap is the point. Money the household holds but no account claims is
 * shown as its own line, because a total that quietly absorbed it would answer
 * "how much is in M-Pesa" with a number that was partly nowhere.
 */
function AccountsCard(props: { feed: FeedDto; onChanged: () => void }) {
  const [placing, setPlacing] = createSignal(false);
  const [failed, setFailed] = createSignal<string | null>(null);
  const [target, setTarget] = createSignal<string | null>(null);

  const gap = () => props.feed.unaccounted;
  const accounts = () => props.feed.accounts;

  /** Say where the already-counted money actually sits. Moves no money. */
  const place = async (account: string | null) => {
    setPlacing(true);
    setFailed(null);
    try {
      const r = await engine.confirm(
        props.feed.sustainId,
        "budget.place_unaccounted",
        account ? { account } : {},
        null,
        null,
      );
      if (r.verdict !== "admitted") {
        setFailed(r.reason ?? "the engine refused it");
        return;
      }
      setTarget(null);
      props.onChanged();
    } catch (e) {
      setFailed(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setPlacing(false);
    }
  };

  return (
    <Show when={accounts().length > 0 || Math.abs(gap()) >= 1}>
      <div class={O.card}>
        <h2 class={O.cardTitle}>where your money is</h2>

        <For each={accounts()}>
          {(a) => (
            <>
              <div class={O.row}>
                <span class={O.figureLabel}>{a.label}</span>
                <span class={O.caption}>{fmt(a.balance)}</span>
              </div>
              {/* ★★★ The bank's own word, next to ours. Reported, never
                  corrected: a difference is a real thing to look into, and
                  quietly moving our figure to match would erase whatever
                  caused it. A shilling of rounding is not news. */}
              <Show when={a.drift !== null && Math.abs(a.drift ?? 0) >= 1}>
                <p class={O.caption}>
                  {a.label} itself last said {fmt(a.reported)}, which is{" "}
                  {fmt(Math.abs(a.drift ?? 0))}{" "}
                  {(a.drift ?? 0) > 0 ? "more" : "less"} than we have accounted for.
                  Probably a text that was never sorted.
                </p>
              </Show>
            </>
          )}
        </For>

        {/* ★★★ Not folded into a total. This is money he holds that nothing
            says the location of, and the only honest thing is to ask. */}
        <Show when={Math.abs(gap()) >= 1}>
          <p class={O.caption}>
            {fmt(gap())} is counted but not yet in an account. Which one is it in?
          </p>
          <Show
            when={target() !== null}
            fallback={
              <div class={O.row}>
                <button class={O.linkish} onClick={() => setTarget("mpesa")}>
                  M-Pesa
                </button>
                <button class={O.linkish} onClick={() => setTarget("kcb")}>
                  KCB
                </button>
                <button class={O.linkish} onClick={() => void place(null)}>
                  not sure yet
                </button>
                {/* "not sure yet" is a real answer: it gets a named holding he
                    can see and move later, rather than staying nowhere. */}
              </div>
            }
          >
            <div class={O.row}>
              <button
                class={O.linkish}
                disabled={placing()}
                onClick={() => void place(target())}
              >
                {placing() ? "placing…" : `put ${fmt(gap())} in ${target()}`}
              </button>
              <button class={O.linkish} onClick={() => setTarget(null)}>
                cancel
              </button>
            </div>
          </Show>
          <Show when={failed()}>
            <p class={O.caption}>{failed()}</p>
          </Show>
        </Show>
      </div>
    </Show>
  );
}

/** The health hue for a card's leading edge, or nothing when unknown. */
function healthEdge(feed: FeedDto): string {
  switch (feed.trend?.health) {
    case "red":
      return O.healthRed;
    case "amber":
      return O.healthAmber;
    case "green":
      return O.healthGreen;
    default:
      return "";
  }
}

function Summary(props: { feed: FeedDto }) {
  const total = () =>
    props.feed.rollup?.aggregates.find((a) => a.childPath === "finances.liquid.balance");
  const members = () => total()?.included.filter((c) => !c.isHousehold).length ?? 0;

  return (
    <div class={`${O.card} ${healthEdge(props.feed)}`}>
      <span class={O.figureLabel}>
        {total()?.grounded ? "everything, together" : "liquid"}
      </span>
      <span class={O.figure}>
        {total()?.grounded ? fmt(total()?.value ?? null) : fmt(props.feed.liquid)}
      </span>
      <Show when={total()?.grounded}>
        <p class={O.caption}>
          across {members()} member{members() === 1 ? "" : "s"} and the household itself
        </p>
        {/* ★★★ An exclusion is never silent. A figure that quietly counted an
            unreadable member as zero would read as calm and be wrong. */}
        <Show when={(total()?.excluded.length ?? 0) > 0}>
          <p class={O.caption}>
            {total()?.excluded.length} left out of the total:{" "}
            {total()
              ?.excluded.map((e) => e.label)
              .join(", ")}
          </p>
        </Show>
      </Show>
    </div>
  );
}

/** ★★★ "Why am I seeing this?" — real numbers, never a rationalisation. */
function Why(props: {
  card: {
    urgency: number;
    measured: boolean;
    basis: string;
    relevance: number;
    score: number;
    cost: number;
    eligibility: string;
  };
}) {
  const [open, setOpen] = createSignal(false);
  return (
    <>
      <button class={O.linkish} onClick={() => setOpen((o) => !o)}>
        {open() ? "▾" : "▸"} why am I seeing this?
      </button>
      <Show when={open()}>
        <div class={O.stack}>
          <p class={O.caption}>{props.card.eligibility}</p>
          {/* ★★ A 0 on an undeclared basis is silence, not safety. */}
          <p class={O.caption}>
            {props.card.measured
              ? `urgency ${props.card.urgency.toFixed(2)} · ${props.card.basis}`
              : `urgency not measured · ${props.card.basis}`}
          </p>
          <p class={O.caption}>
            relevance {props.card.relevance.toFixed(2)} · score {props.card.score.toFixed(2)} ·
            costs {props.card.cost} of the attention budget
          </p>
        </div>
      </Show>
    </>
  );
}

/** What a card draws. ★ Keyed off its declared `render` tag. */
function CardBody(props: { card: { id: string; render: string }; feed: FeedDto }) {
  const fraction = () => {
    const allocated = num(props.feed, "worst_allocated");
    return allocated > 0 ? num(props.feed, "worst_spent") / allocated : null;
  };

  return (
    <Show
      when={props.card.render === "pocket_strain"}
      fallback={
        <Show
          when={props.card.render === "person_tab"}
          fallback={
            <Show
              when={props.card.render === "net_worth"}
              fallback={<PlainCard feed={props.feed} card={props.card} />}
            >
              <NetWorthCard feed={props.feed} />
            </Show>
          }
        >
          <PersonTabCard feed={props.feed} />
        </Show>
      }
    >
      <span class={O.figureLabel}>{str(props.feed, "worst_pocket")}</span>
      <span class={O.figure}>{fmt(num(props.feed, "worst_spent"))}</span>
      <p class={O.caption}>of {fmt(num(props.feed, "worst_allocated"))} set aside</p>
      <div class={O.meter}>
        <div
          class={O.meterFill}
          style={{
            width: `${Math.min(100, (fraction() ?? 0) * 100)}%`,
            background: (fraction() ?? 0) > 0.9 ? "#e05050" : "#E8A020",
          }}
        />
      </div>
      <p class={O.caption}>the limit is the one you set for it</p>
    </Show>
  );
}

/**
 * A pocket that is a PERSON, drawn as a relationship.
 *
 * ★★★ The same numbers as any pocket would be a lie of omission. KES 1,000
 * outstanding reads identically whether he sent 1,000 once or sent 40,000
 * across a year and got 39,000 back, and those are not the same relationship.
 * Both sides, always — the net alone is the thing that hides it.
 *
 * ★★★ It says WHO OWES WHOM in words, not as a signed number. A negative
 * balance is a convention the ledger uses and a person has to decode; "you owe
 * her" is the fact itself. Getting the sign backwards on a screen is how
 * somebody pays a debt that was never theirs.
 *
 * ★★ The number is shown masked. He linked it as an identifier, and the pocket
 * name already says which person this is — the digits are only there to tell
 * two people apart if he ever names two pockets alike.
 */
function PersonTabCard(props: { feed: FeedDto }) {
  const sent = () => num(props.feed, "tab_sent");
  const received = () => num(props.feed, "tab_received");
  const out = () => num(props.feed, "tab_out");
  const allocated = () => num(props.feed, "tab_allocated");
  // ★ Rounded, because a tab settled to the last shilling should read as
  //   settled rather than as a rounding artefact of two large sides.
  const settled = () => Math.abs(out()) < 0.5;

  return (
    <>
      <span class={O.figureLabel}>{str(props.feed, "tab_person")} · person</span>
      <span class={O.figure}>{fmt(Math.abs(out()))}</span>
      <p class={O.caption}>
        {settled()
          ? "square — nothing outstanding either way"
          : out() > 0
            ? `${str(props.feed, "tab_person")} owes you`
            : `you owe ${str(props.feed, "tab_person")}`}
      </p>

      {/* ★★★ The two sides, always both. This is the whole reason the card
          exists rather than reusing the pocket one. */}
      <div class={O.row}>
        <span class={O.caption}>you sent {fmt(sent())}</span>
        <span class={O.spacer} />
        <span class={O.caption}>they sent {fmt(received())}</span>
      </div>

      <Show when={allocated() > 0}>
        <div class={O.meter}>
          <div
            class={O.meterFill}
            style={{
              width: `${Math.min(100, Math.max(0, (out() / allocated()) * 100))}%`,
              background: out() > allocated() ? "#e05050" : "#E8A020",
            }}
          />
        </div>
        <p class={O.caption}>
          of {fmt(allocated())} set aside for them
          {out() > allocated() ? " — you have gone past it" : ""}
        </p>
      </Show>

      <p class={O.caption}>
        tied to {str(props.feed, "tab_number")} · both directions land here
      </p>
    </>
  );
}

/**
 * What the household is worth, not what is in the bank.
 *
 * ★★★ Four lines, never one number. Money out is not money gone — a week's
 * shopping becomes food in the cupboard, a loan becomes a claim — and a single
 * total hides exactly the distinction that makes it true. A household that
 * shops well and one that loses money have the same bank balance and very
 * different positions.
 *
 * ★★ Lines at zero are not drawn. A household with nothing lent out should not
 * be shown a row of noughts to read past on the way to the number it wanted.
 */
function NetWorthCard(props: { feed: FeedDto }) {
  const line = (label: string, key: string) => {
    const v = num(props.feed, key);
    return (
      <Show when={Math.abs(v) > 0.005}>
        <div class={O.row}>
          <span class={O.caption}>{label}</span>
          <span class={O.spacer} />
          <span class={O.caption}>{fmt(v)}</span>
        </div>
      </Show>
    );
  };

  return (
    <>
      <span class={O.figureLabel}>worth</span>
      <span class={O.figure}>{fmt(num(props.feed, "worth"))}</span>
      {line("in accounts", "worth_cash")}
      {line("things you hold", "worth_things")}
      {line("owed to you", "worth_owed_to_you")}
      {line("you owe", "worth_owed_by_you")}
      <p class={O.caption}>what you hold, not what you have left to spend</p>
    </>
  );
}

function PlainCard(props: { feed: FeedDto; card: { id: string } }) {
  return (
    <Show
      when={props.card.id === "classify_capture"}
      fallback={
        <>
          <span class={O.figureLabel}>liquid</span>
          <span class={O.figure}>{fmt(num(props.feed, "liquid"))}</span>
        </>
      }
    >
      <span class={O.figureLabel}>waiting on you</span>
      <span class={O.figure}>{str(props.feed, "unclassified")}</span>
      <p class={O.caption}>each one needs a pocket before it can be recorded</p>
    </Show>
  );
}

/**
 * ★★★ The signature flow: an effect → `infer` → **one question at a time** →
 * confirm → the operator through the real gate.
 *
 * ★★ `autoStart` matters more than it looks. Before, a captured message showed
 * a "classify this" button whose only job was to trigger the inference the
 * screen was always going to need — a tap, and a wait, that bought nothing. On
 * a card that exists *because* something needs classifying, the first question
 * should already be on screen when you get there.
 */
/**
 * What the money turned into.
 *
 * ★★★ "Food, KES 90" records that money left. It does not record that beans,
 * onions and carrots arrived — and those are real things the household now
 * holds. Without them the only account of the week is money going out, which
 * makes a household that shops well look identical to one that loses money.
 *
 * ★★ Offered, never required. Most spends are not baskets, and a step that
 * had to be dismissed every time would cost more attention than it earns.
 */
function Itemize(props: {
  sustain: string;
  pocket: string;
  limit: number;
  sourceTx: string;
  onDone: () => void;
}) {
  const [open, setOpen] = createSignal(false);
  const [lines, setLines] = createSignal<{ item: string; value: string; subpocket: string }[]>([
    { item: "", value: "", subpocket: "" },
  ]);
  /**
   * ★★★ Grouping is off until asked for.
   *
   * Most shopping is a flat list and a third field on every row would tax
   * every basket to serve the few that want grouping. The operator has always
   * stored a subpocket per line; this is the switch that lets him fill it in.
   */
  const [grouped, setGrouped] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [failed, setFailed] = createSignal<string | null>(null);
  const [done, setDone] = createSignal<{ count: number; unlisted: number } | null>(null);

  const listed = () =>
    lines().reduce((t, l) => t + (Number.parseFloat(l.value) || 0), 0);
  const left = () => Math.round((props.limit - listed()) * 100) / 100;
  const usable = () =>
    lines().filter((l) => l.item.trim() !== "" && (Number.parseFloat(l.value) || 0) > 0);

  const set = (i: number, k: "item" | "value" | "subpocket", v: string) =>
    setLines((ls) => ls.map((l, n) => (n === i ? { ...l, [k]: v } : l)));

  const save = async () => {
    const rows = usable();
    if (rows.length === 0) return;
    setBusy(true);
    setFailed(null);
    try {
      const r = await engine.confirm(
        props.sustain,
        "inventory.itemize",
        {
          pocket_name: props.pocket,
          source_tx: props.sourceTx,
          limit: props.limit,
          lines: rows.map((l) => ({
            item: l.item.trim(),
            value: Number.parseFloat(l.value),
            // Omitted rather than sent empty: an empty group is not a group.
            ...(l.subpocket.trim() !== "" ? { subpocket: l.subpocket.trim() } : {}),
          })),
        },
        null,
        null,
      );
      if (r.verdict !== "admitted") {
        setFailed(r.reason ?? "the engine refused it");
        return;
      }
      const d = r.data as { count?: number; unlisted?: number } | null;
      setDone({ count: d?.count ?? rows.length, unlisted: d?.unlisted ?? 0 });
      props.onDone();
    } catch (e) {
      setFailed(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Show when={done()} fallback={
      <Show
        when={open()}
        fallback={
          <button class={O.linkish} onClick={() => setOpen(true)}>
            + what did this buy?
          </button>
        }
      >
        <div class={O.stack}>
          <p class={O.caption}>
            What {fmt(props.limit)} in {props.pocket} brought in. {fmt(left())} left to account
            for — anything you leave out just stays a plain spend.
          </p>
          <For each={lines()}>
            {(l, i) => (
              <div class={O.row}>
                <input
                  class={O.input}
                  placeholder="what"
                  value={l.item}
                  onInput={(e) => set(i(), "item", e.currentTarget.value)}
                />
                <input
                  class={O.inputNumeric}
                  placeholder="how much"
                  inputmode="decimal"
                  value={l.value}
                  onInput={(e) => set(i(), "value", e.currentTarget.value)}
                />
                <Show when={grouped()}>
                  <input
                    class={O.input}
                    placeholder="group"
                    value={l.subpocket}
                    onInput={(e) => set(i(), "subpocket", e.currentTarget.value)}
                  />
                </Show>
              </div>
            )}
          </For>
          <div class={O.row}>
            <button
              class={O.linkish}
              onClick={() => setLines((ls) => [...ls, { item: "", value: "", subpocket: "" }])}
            >
              + another
            </button>
            {/* ★★ Subpockets: Food into Vegetables and Grains. The card says
                what it is for rather than naming the concept, because "group
                these" is the thing he wants and "subpocket" is our word. */}
            <button class={O.linkish} onClick={() => setGrouped((g) => !g)}>
              {grouped() ? "no grouping" : "group these"}
            </button>
            <span class={O.spacer} />
            <button class={O.linkish} onClick={() => setOpen(false)}>
              not now
            </button>
          </div>
          {/* ★★ Refused before it is sent when the list already overruns, so
              he is not told no by the engine for something visible here. */}
          <Show when={left() < 0}>
            <p class={O.caption}>
              That is {fmt(Math.abs(left()))} more than was spent.
            </p>
          </Show>
          <Show when={failed()}>{(f) => <div class={O.errorBox}>{f()}</div>}</Show>
          <button
            class={`${O.action.primary} ${O.actionWide}`}
            disabled={busy() || usable().length === 0 || left() < 0}
            onClick={() => void save()}
          >
            {busy() ? "adding…" : `add ${usable().length || ""} to what you hold`}
          </button>
        </div>
      </Show>
    }>
      {(d) => (
        <p class={O.caption}>
          {d().count} {d().count === 1 ? "thing" : "things"} added to what you hold.
          <Show when={d().unlisted > 0}>
            {" "}
            {fmt(d().unlisted)} of it stays a plain spend.
          </Show>
        </p>
      )}
    </Show>
  );
}

/**
 * **The classifier.** One definition, several doors.
 *
 *(*) **Exported deliberately.** This is reached from the Orchie feed, from a
 * notification tap, and from the Ingest message list -- and it must be the SAME
 * flow in all three, not three surfaces that agree today. A second classifier
 * would drift the moment one of them learned something the others did not, and
 * what drifts is what a household believes about its own money.
 *
 * It renders a self-contained stack and owns all its own state, so a caller
 * supplies the subject and is told when it is finished. Nothing about where it
 * was opened from changes what it does.
 */
export function Classify(props: {
  sustain: string;
  messageId: string | null;
  autoStart?: boolean;
  /**
   * Sorting texts from the past rather than filing one that just arrived.
   *
   * ★★ It changes which action leads, and nothing else. For a past text the
   * money already moved in the world, so recording both sides is the honest
   * default; for a live one, funding the pocket and then spending is.
   */
  backfill?: boolean;
  /**
   * Which pockets are people rather than envelopes.
   *
   * ★★ Passed in rather than fetched: the feed already knows, and a second
   * round trip per card to learn something the screen was handed would be work
   * for nothing.
   */
  personPockets?: string[];
  onDone: () => void;
}) {
  const [text, setText] = createSignal("");
  const [known, setKnown] = createSignal<Record<string, JsonValue>>({});
  const [ignoreHistory, setIgnoreHistory] = createSignal(false);
  const [step, setStep] = createSignal<InferenceDto | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [verdict, setVerdict] = createSignal<GateResult | null>(null);
  const [learned, setLearned] = createSignal<string | null>(null);
  // ★★ The params a person actually confirmed. Learning synthesises a rule from
  //    THESE — the confirmed amount has to be findable in the message text, so
  //    sending an empty object would teach nothing and say it learned.
  const [confirmed, setConfirmed] = createSignal<JsonValue | null>(null);
  const [confirmedOp, setConfirmedOp] = createSignal<string | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);
  // ★★★ Optimistic: the option a thumb just landed on, held lit while the
  //     engine answers. Without this a tap produced NOTHING visible until the
  //     round trip returned, which is the single biggest reason it felt dead.
  const [pending, setPending] = createSignal<string | null>(null);
  // How many questions have been answered — a short trail, so the flow has a
  // visible bottom to it rather than feeling like it could go on forever.
  const [answered, setAnswered] = createSignal(0);

  const run = async (next: Record<string, JsonValue>) => {
    setFailure(null);
    setBusy(true);
    try {
      setKnown(next);
      setStep(
        await engine.infer(
          props.sustain,
          props.messageId,
          props.messageId ? null : text() || null,
          next as JsonValue,
          ignoreHistory(),
        ),
      );
    } catch (e) {
      setFailure(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
      setPending(null);
    }
  };

  const [skipping, setSkipping] = createSignal(false);

  /**
   * Set this message aside. Not a transaction.
   *
   * ★★ A reversal, a promo, a notice: nothing to file. The message keeps its
   * place in the log with its text and a real "set aside" mark, so this is
   * never a disappearance nobody can account for.
   */
  const skip = async () => {
    const id = props.messageId;
    if (!id) return;
    setSkipping(true);
    setFailure(null);
    try {
      await engine.ignoreMessage(id);
      props.onDone();
    } catch (e) {
      setFailure(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setSkipping(false);
    }
  };

  /**
   * "Never ask me about these again."
   *
   * ★★★ Across a backlog this size the same handful of shapes repeat, and
   * setting each aside one at a time is the same decision made hundreds of
   * times. Someone who has already answered it twice is right to be annoyed
   * the third time.
   *
   * ★★ It clears what is already waiting, not only what arrives next: the
   * question is being answered in the middle of the pile it is meant to clear.
   */
  const [skipLearned, setSkipLearned] = createSignal<SkipLearnedDto | null>(null);

  /**
   * ★★ Offered for any captured message, and answered by the engine.
   *
   * Whether a shape can be learned at all depends on whether a rule recognised
   * it, which only the engine knows. Hiding the button would mean guessing
   * that here; offering it and letting the answer come back explains WHY an
   * unreadable message cannot teach a rule, which is worth more than a button
   * that quietly is not there.
   */
  const skipAllLikeThis = async () => {
    const id = props.messageId;
    if (!id) return;
    setSkipping(true);
    setFailure(null);
    try {
      const out = await engine.learnSkip(props.sustain, id);
      setSkipLearned(out);
      if (!out.unlearnable) props.onDone();
    } catch (e) {
      setFailure(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setSkipping(false);
    }
  };

  /**
   * The spend that just landed, if what landed was a spend.
   *
   * ★★ Read off the params he actually confirmed, not off the inference —
   * an inference can be edited before it is sent, and it is the confirmed
   * call that moved the money and therefore bought the things.
   *
   * ★★★ `budget.allocate` is deliberately excluded. Moving money into a
   * pocket buys nothing, so offering to itemize it would be asking what a
   * transfer between his own intentions brought home.
   */
  const spentJustNow = () => {
    // Only a spend buys anything.
    if (confirmedOp() !== "budget.spend") return null;
    const p = confirmed() as Record<string, unknown> | null;
    if (!p) return null;
    const pocket = typeof p.pocket_name === "string" ? p.pocket_name : null;
    const amount = typeof p.amount === "number" ? p.amount : null;
    const tx = props.messageId ?? null;
    if (!pocket || !amount || !tx) return null;
    return { pocket, amount, sourceTx: tx };
  };

  /** Is the whole pocket list showing, or just the leading few? */
  const [allPockets, setAllPockets] = createSignal(false);

  /**
   * The options actually rendered as buttons.
   *
   * ★ Only trims the POCKET question. Every other question offers a small,
   * fixed set that is already the whole answer.
   */
  const shortlist = (opts: ChoiceDto[], field: string) =>
    field === "pocket_name" && !allPockets() ? opts.slice(0, SUGGESTIONS) : opts;

  const hidden = (opts: ChoiceDto[], field: string) =>
    field === "pocket_name" ? Math.max(0, opts.length - SUGGESTIONS) : 0;

  /** The inline pocket creator: closed, or open with a name being typed. */
  const [newPocket, setNewPocket] = createSignal<string | null>(null);
  const [creating, setCreating] = createSignal(false);
  const [createFailed, setCreateFailed] = createSignal<string | null>(null);

  /**
   * Make a pocket, then file into it.
   *
   * ★★★ Through `budget.add_pocket`, the real Enzyme, so it passes the same
   * gate as everything else and lands in the log. The engine normalises the
   * name, so the pocket to answer with is read back off the resulting state
   * rather than guessed at here: two normalisers would drift.
   */
  const createAndFile = async (field: string, existing: string[]) => {
    const name = (newPocket() ?? "").trim();
    if (name === "") return;
    setCreating(true);
    setCreateFailed(null);
    try {
      // ★ The options ARE the pockets he has, so the new one is whichever name
      //   the engine's state gained. No second call, and no second normaliser.
      const before = new Set(existing);
      const r = await engine.confirm(
        props.sustain,
        "budget.add_pocket",
        { pocket_name: name },
        null,
        null,
      );
      if (r.verdict !== "admitted") {
        setCreateFailed(r.reason ?? "the engine refused it");
        return;
      }
      const made = pocketNames(r.state).find((p) => !before.has(p));
      setNewPocket(null);
      answer(field, made ?? name);
    } catch (e) {
      setCreateFailed(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setCreating(false);
    }
  };

  const answer = (field: string, value: string) => {
    setPending(value);
    setAnswered((n) => n + 1);
    void run({ ...known(), [field]: value });
  };

  const reset = () => {
    setStep(null);
    setKnown({});
    setAnswered(0);
    setIgnoreHistory(false);
    setPending(null);
  };

  const confirm = async (s: Extract<InferenceDto, { status: "ready" }>) => {
    setBusy(true);
    setFailure(null);
    try {
      const v = await engine.confirm(
        props.sustain,
        s.operator,
        s.params,
        props.messageId,
        s.description,
      );
      setVerdict(v);
      setConfirmed(s.params as JsonValue);
      // ★ Which operator ran, not only with what. A spend and an allocation
      //   carry identical params and mean opposite things about whether
      //   anything was bought.
      setConfirmedOp(s.operator);
      if (v.verdict === "admitted") {
        reset();
        setText("");
        props.onDone();
      }
    } catch (e) {
      setFailure(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  };

  const [fixing, setFixing] = createSignal<null | "fund" | "backfill">(null);
  const [fixFailed, setFixFailed] = createSignal<string | null>(null);

  /**
   * Put money in the pocket, then record the spend.
   *
   * ★★★ Two real Enzymes through the real gate, in order, with no bypass.
   * `budget.allocate` moves liquid into the pocket and `budget.spend` files
   * against it, which is the same path a person would take by hand.
   *
   * The two differ only in how much they move:
   *   backfill  the shortfall, so the pocket ends exactly consumed. For a text
   *             from the past, where the money already moved in the world.
   *   fund      the whole amount, so the pocket keeps room for the next one.
   * On an empty pocket those are the same number, and the labels still say
   * which thing he is doing.
   */
  const fixAndRecord = async (room: NoRoom, how: "fund" | "backfill") => {
    const s = step();
    if (!s || s.status !== "ready") return;
    const move = how === "backfill" ? room.shortfall : room.requested;
    setFixing(how);
    setFixFailed(null);
    try {
      // The allocation is recorded against the message but does not finish
      // it: the spend that follows is what deals with it. Both are on the
      // record, so a refund of this charge knows the money came out of liquid
      // and can put it back there.
      const put = await engine.confirm(
        props.sustain,
        "budget.allocate",
        { pocket_name: room.pocket, amount: move },
        props.messageId,
        null,
        false,
      );
      if (put.verdict !== "admitted") {
        // ★ Most likely liquid itself is short. The engine says so; this does
        //   not invent a second explanation.
        setFixFailed(put.reason ?? "the engine refused the allocation");
        return;
      }
      await confirm(s);
    } catch (e) {
      setFixFailed(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setFixing(null);
    }
  };

  /** Back to the picker, keeping everything except the pocket. */
  const pickAgain = () => {
    setVerdict(null);
    setFixFailed(null);
    const { pocket_name: _drop, ...rest } = known();
    setKnown(rest);
    void run(rest);
  };

  /** Is this pocket somebody rather than something? */
  const isPerson = (pocket: string) => (props.personPockets ?? []).includes(pocket);

  const remember = async (operator: string, params: JsonValue) => {
    if (!props.messageId) return;
    try {
      setLearned(await engine.learnRule(props.messageId, operator, params));
    } catch (e) {
      setFailure(String(e).replace(/^Error:\s*/, ""));
    }
  };

  /**
   * ★★★ Arming is a function of the SUBJECT, not of when this happened to
   * mount.
   *
   * It used to be `onMount`, which fires once. Any reuse of the instance for a
   * second message therefore produced a card with no actions on it. An effect
   * on the message id cannot have that failure: change the subject and the
   * card asks about the new subject, whether or not it was re-created.
   */
  createEffect(() => {
    const id = props.messageId;
    if (!props.autoStart || !id) return;
    // ★ Only the id is tracked. `untrack` keeps the clearing below from
    //   feeding back into this effect and looping.
    untrack(() => {
      reset();
      void run({});
    });
  });

  return (
    <div class={O.stack}>
      {/* ── narration entry (only when there is no captured message) ───── */}
      <Show when={!props.messageId && !step() && !verdict()}>
        <input
          ref={keyboardAware}
          class={O.input}
          type="text"
          enterkeyhint="go"
          autocapitalize="none"
          autocomplete="off"
          placeholder="spent 500 on food"
          value={text()}
          onInput={(e) => setText(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.currentTarget.blur();
              void run({});
            }
          }}
        />
        <button
          class={`${O.action.primary} ${O.actionWide}`}
          onClick={() => void run({})}
          disabled={busy() || text().trim() === ""}
        >
          <Show when={busy()} fallback="work it out">
            <span class={O.working} /> working it out…
          </Show>
        </button>
      </Show>

      {/* ── a captured message that is not auto-started ────────────────── */}
      <Show when={props.messageId && !props.autoStart && !step() && !verdict()}>
        <button
          class={`${O.action.primary} ${O.actionWide}`}
          onClick={() => void run({})}
          disabled={busy()}
        >
          <Show when={busy()} fallback="classify this">
            <span class={O.working} /> working it out…
          </Show>
        </button>
      </Show>

      {/* ── the first inference on an auto-started card ────────────────── */}
      <Show when={props.autoStart && !step() && busy() && !verdict()}>
        <div class={O.stack}>
          <div class={O.skelLine.text} />
          <div class={O.options}>
            <div class={O.skeleton} style={{ height: "56px" }} />
            <div class={O.skeleton} style={{ height: "56px" }} />
          </div>
        </div>
      </Show>

      <Show when={step()}>
        {(s) => (
          <div class={O.stack}>
            {/* ═══ one question at a time ═══════════════════════════════ */}
            <Show when={s().status === "needsDisambiguation"}>
              {(() => {
                const q = () => s() as Extract<InferenceDto, { status: "needsDisambiguation" }>;
                return (
                  <div class={O.stack}>
                    <Show when={answered() > 0}>
                      <div class={O.steps}>
                        <For each={Array.from({ length: answered() })}>
                          {() => <span class={O.stepDot.done} />}
                        </For>
                        <span class={O.stepDot.todo} />
                      </div>
                    </Show>

                    <h3 class={O.cardTitle}>{q().question}</h3>
                    <Show when={q().why}>
                      <p class={O.caption}>{q().why}</p>
                    </Show>

                    {/* ★ options: null means the answer is not a tap. */}
                    <Show
                      when={q().options}
                      fallback={
                        <>
                          <input
                            ref={keyboardAware}
                            class={O.inputNumeric}
                            // ★★ `decimal` gives a numeric pad WITH a decimal
                            //    separator; `numeric` gives digits only, which
                            //    makes 1500.50 impossible to type.
                            inputmode="decimal"
                            enterkeyhint="done"
                            placeholder="0"
                            onKeyDown={(e) => {
                              if (e.key === "Enter") {
                                const v = e.currentTarget.value;
                                e.currentTarget.blur();
                                answer(q().field, v);
                              }
                            }}
                          />
                          <p class={O.caption}>type the amount, then press done</p>
                        </>
                      }
                    >
                      {(opts) => (
                        <div class={manyToRead(opts()) ? O.chips : O.options}>
                          {/* ★★★ A few, then the rest on request.
                              Every pocket as a full-width button stops fitting
                              somewhere around ten and asks him to read the lot
                              before choosing. The engine orders them by how
                              often he has actually used each one, so the head
                              of the list is the answer most of the time and
                              the tail is one tap away. */}
                          <For each={shortlist(opts(), q().field)}>
                            {(o) => {
                              const small = manyToRead(opts());
                              const chosen = pending() === o.value;
                              return (
                                <button
                                  class={
                                    small
                                      ? chosen
                                        ? O.chipChosen
                                        : isPerson(o.value)
                                          ? O.chipPerson
                                          : O.chip
                                      : chosen
                                        ? O.optionChosen
                                        : O.option
                                  }
                                  disabled={busy()}
                                  onClick={() => answer(q().field, o.value)}
                                >
                                  {/* ★★ In the roomy layout the marker is a
                                      word, because there is space for one and a
                                      word cannot be mistaken for decoration. */}
                                  {o.label}
                                  <Show when={!small && isPerson(o.value)}>
                                    <span class={O.caption}> · a person, both ways</span>
                                  </Show>
                                </button>
                              );
                            }}
                          </For>
                          <Show when={hidden(opts(), q().field) > 0 && !allPockets()}>
                            <button class={O.linkish} onClick={() => setAllPockets(true)}>
                              more pockets ({hidden(opts(), q().field)})
                            </button>
                          </Show>
                          {/* ★★ A pocket he does not have yet is a real answer.
                              Without this the only way out of a wrong guess is
                              to accept it. */}
                          <Show when={q().field === "pocket_name"}>
                            <Show
                              when={newPocket() !== null}
                              fallback={
                                <button
                                  class={O.chipNew}
                                  disabled={busy()}
                                  onClick={() => setNewPocket("")}
                                >
                                  + new
                                </button>
                              }
                            >
                              <input
                                ref={keyboardAware}
                                class={O.input}
                                placeholder="name it"
                                enterkeyhint="done"
                                value={newPocket() ?? ""}
                                onInput={(e) => setNewPocket(e.currentTarget.value)}
                                onKeyDown={(e) => {
                                  if (e.key === "Enter") {
                                    e.currentTarget.blur();
                                    void createAndFile(q().field, opts().map((o) => o.value));
                                  }
                                }}
                              />
                              <button
                                class={O.option}
                                disabled={creating() || (newPocket() ?? "").trim() === ""}
                                onClick={() => void createAndFile(q().field, opts().map((o) => o.value))}
                              >
                                {creating() ? "making…" : "make it and file here"}
                              </button>
                              <button class={O.linkish} onClick={() => setNewPocket(null)}>
                                cancel
                              </button>
                            </Show>
                            {/* ★ A refusal says why. "Pocket 'x' already
                                exists" is the engine's own wording. */}
                            <Show when={createFailed()}>
                              {(f) => <div class={O.errorBox}>{f()}</div>}
                            </Show>
                          </Show>
                        </div>
                      )}
                    </Show>

                    {/* ★★ No "start over" here. It cleared the answers and
                        left the card with nothing to ask, which is a dead end
                        rather than a way back — and every real way out is
                        already on this card: pick a different pocket, make a
                        new one, record it, or set it aside. `reset` itself
                        stays; it is what runs after a successful record. */}
                    <div class={O.row}>
                      <span class={O.spacer} />
                      {/* ★ Quiet on purpose. Most messages ARE transactions,
                          so this is the exception, not a peer of recording. */}
                      <Show when={props.messageId}>
                        <button
                          class={O.linkish}
                          disabled={skipping()}
                          onClick={() => void skip()}
                        >
                          {skipping() ? "setting aside…" : "not a transaction · skip"}
                        </button>
                        {/* ★★★ The same answer, given once for the whole
                            shape rather than once per message. This is the
                            one that saves real time on a backlog. */}
                        <button
                          class={O.linkish}
                          disabled={skipping()}
                          onClick={() => void skipAllLikeThis()}
                        >
                          skip all like this
                        </button>
                      </Show>
                      {/* ★★★ A queue that quietly shrank by forty would be
                          alarming rather than helpful. */}
                      <Show when={skipLearned()}>
                        {(l) => (
                          <p class={O.caption}>
                            {l().unlearnable
                              ? "Nothing to learn from this one — Orchie could not read it, and those are the ones worth your eyes."
                              : `Set aside, and ${l().cleared} more like it. New ones like this will not ask again.`}
                          </p>
                        )}
                      </Show>
                    </div>
                  </div>
                );
              })()}
            </Show>

            {/* ═══ ready to record ══════════════════════════════════════ */}
            <Show when={s().status === "ready"}>
              {(() => {
                const rd = () => s() as Extract<InferenceDto, { status: "ready" }>;
                return (
                  <div class={O.stack}>
                    {/* ★★ Plain, from the params themselves. The engine's own
                        `why` names the operator and its arguments, which is
                        right in the cockpit and noise here. */}
                    <p class={O.body}>{plainly(rd().operator, rd().params)}</p>

                    {/* ★★ A history pre-fill is never silent. */}
                    <Show when={rd().fromHistory}>
                      <div class={O.row}>
                        <span class={O.badge.quiet}>from what you did before</span>
                        <span class={O.spacer} />
                        <button
                          class={O.linkish}
                          onClick={() => {
                            setIgnoreHistory(true);
                            const { pocket_name: _drop, ...rest } = known();
                            void run(rest);
                          }}
                        >
                          change
                        </button>
                      </div>
                    </Show>

                    <button
                      class={`${O.action.primary} ${O.actionWide}`}
                      onClick={() => void confirm(rd())}
                      disabled={busy()}
                    >
                      <Show when={busy()} fallback="record it">
                        <span class={O.working} /> working…
                      </Show>
                    </button>
                    <button class={`${O.action.quiet} ${O.actionWide}`} onClick={reset}>
                      not now
                    </button>
                  </div>
                );
              })()}
            </Show>

            <Show when={s().status === "cannotInfer"}>
              <p class={O.body}>
                {(s() as Extract<InferenceDto, { status: "cannotInfer" }>).why}
              </p>
              <button class={`${O.action.quiet} ${O.actionWide}`} onClick={reset}>
                try again
              </button>
            </Show>
          </div>
        )}
      </Show>

      {/* ═══ the gate's answer ══════════════════════════════════════════ */}
      <Show when={verdict()}>
        {(v) => (
          <div class={O.stack}>
            <div class={O.verdict[v().verdict === "admitted" ? "admitted" : "refused"]}>
              <div class={O.verdictWord[v().verdict === "admitted" ? "admitted" : "refused"]}>
                {v().verdict === "admitted" ? "recorded" : "not yet"}
              </div>
              {/* ★★★ A pocket with no room is not a dead end, it is a fork.
                  The operator hands back `remaining` and `shortfall` for
                  exactly this, so the reason sits ABOVE real buttons. */}
              <Show
                when={noRoom(v())}
                fallback={
                  <>
                    <Show when={v().reason}>{(r) => <p class={O.caption}>{r()}</p>}</Show>
                    <Show when={v().verdict !== "admitted"}>
                      <p class={O.caption}>nothing moved, nothing recorded</p>
                    </Show>
                    {/* ★★★ Only after a spend really landed, and only for a
                        spend: allocating money into a pocket buys nothing, so
                        there is nothing it could have brought in. */}
                    <Show when={v().verdict === "admitted" && spentJustNow()}>
                      {(s) => (
                        <Itemize
                          sustain={props.sustain}
                          pocket={s().pocket}
                          limit={s().amount}
                          sourceTx={s().sourceTx}
                          onDone={props.onDone}
                        />
                      )}
                    </Show>
                  </>
                }
              >
                {(room) => (
                  <div class={O.stack}>
                    {/* ★ Both numbers are the operator's own. Nothing here
                        invents a figure the engine did not report. */}
                    <p class={O.caption}>
                      {room().pocket} has {fmt(room().remaining)} left, and this
                      needs {fmt(room().requested)}.
                    </p>
                    <Show when={fixFailed()}>
                      {(f) => <div class={O.errorBox}>{f()}</div>}
                    </Show>
                    {/* Backfill first: these are texts from the past, and the
                        money already moved in the world. */}
                    {/* ★★ Which one LEADS is the only difference the mode
                        makes. Sorting the backlog, the money already moved, so
                        recording both sides is the honest default. */}
                    <button
                      class={`${props.backfill ? O.action.primary : O.action.secondary} ${O.actionWide}`}
                      disabled={fixing() !== null}
                      onClick={() => void fixAndRecord(room(), "backfill")}
                    >
                      {fixing() === "backfill" ? "recording…" : "record as already spent"}
                    </button>
                    <button
                      class={`${props.backfill ? O.action.secondary : O.action.primary} ${O.actionWide}`}
                      disabled={fixing() !== null}
                      onClick={() => void fixAndRecord(room(), "fund")}
                    >
                      {fixing() === "fund"
                        ? "moving…"
                        : `put ${fmt(room().requested)} into ${room().pocket}, then record`}
                    </button>
                    <button class={O.linkish} onClick={pickAgain}>
                      pick another pocket
                    </button>
                  </div>
                )}
              </Show>
            </div>

            {/* ★★★ Whose tab this is. Offered next to the format memory
                because they answer two different questions about the same
                message: what SHAPE it is, and WHOSE money it was. */}
            <Show when={v().verdict === "admitted" && props.messageId}>
              {(_) => {
                const pocket = () =>
                  (confirmed() as Record<string, JsonValue> | null)?.["pocket_name"];
                return (
                  <Show when={typeof pocket() === "string" && pocket()}>
                    {(p) => (
                      <LinkNumberControl
                        sustain={props.sustain}
                        messageId={props.messageId!}
                        pocket={p() as string}
                      />
                    )}
                  </Show>
                );
              }}
            </Show>

            {/* ★★★ "remember this format" — only after a real success, and only
                for a captured message. A learned SPEND still asks next time. */}
            <Show when={v().verdict === "admitted" && props.messageId}>
              <Show
                when={!learned()}
                fallback={
                  /* ★★★ Say what was actually learned, and say the asymmetry
                     out loud. Since the learner became shape-aware a rule
                     generalises over the amount, the name, the date and the
                     balance — so "recognised from now on" was true but vague,
                     and a person deciding whether to press this deserves to
                     know it will fire on the NEXT message like this one and
                     not only on a repeat of this exact text. And that a spend
                     still asks, because that is the promise they are relying
                     on when they press it. */
                  <p class={O.caption}>
                    learned · the next message shaped like this one is recognised, whatever the
                    amount, the name or the date
                    {v().operator === "budget.record_income"
                      ? " — and income like this files itself"
                      : " — you will still be asked which pocket"}
                  </p>
                }
              >
                <Show when={confirmed()}>
                  {(p) => (
                    <button
                      class={`${O.action.secondary} ${O.actionWide}`}
                      onClick={() => void remember(v().operator, p())}
                    >
                      remember this format
                    </button>
                  )}
                </Show>
              </Show>
            </Show>

            <button
              class={`${O.action.quiet} ${O.actionWide}`}
              onClick={() => {
                setVerdict(null);
                setLearned(null);
                setConfirmed(null);
              }}
            >
              done
            </button>
          </div>
        )}
      </Show>

      <Show when={failure()}>
        {(f) => <div class={O.errorBox}>{f()}</div>}
      </Show>
    </div>
  );
}
