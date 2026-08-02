/**
 * apps/web/src/lib/smsCapture.js
 *
 * JS-side wrapper for the native SmsCapture plugin (Android only,
 * apps/web/android/app/src/main/java/online/vyybandasky/sustena/
 * SmsCapturePlugin.java + SmsReceiver.java). Two real-world sources so
 * far: M-Pesa and KCB Kenya SMS alerts.
 *
 * ARCHITECTURE, and why: native code never talks to the network. It only
 * (a) reads the SMS content provider once for backfill, and (b) queues
 * anything a real-time SMS_RECEIVED broadcast captures into a small
 * SharedPreferences-backed store (SmsReceiver has a short, ~10s execution
 * window before Android may treat it as non-responsive; a single local
 * write comfortably fits that, a real-world mobile HTTP POST is a genuine
 * risk of not). This module is the ONE place that ever calls
 * POST /api/v1/ingest/capture -- through api.post(), so it inherits the
 * real auth header, the real error handling, everything the rest of the
 * app already gets for free.
 *
 * REAL-TIME PATH (native, no JS required): SmsReceiver ALSO enqueues a
 * WorkManager job (IngestWorker.java) directly on SMS arrival, which POSTs
 * the real capture and fires a native "Orchie needs a decision" notification
 * itself, near-immediately, regardless of whether this JS/webview context
 * is even running. This module's own poll/backfill path below is the
 * FALLBACK for whatever the native path couldn't deliver (e.g. no auth
 * context cached natively yet) -- both paths are safe to overlap because
 * capture() is idempotent (dedup_key), so a message the native path already
 * handled just comes back is_duplicate=true here and is correctly skipped
 * (see forwardMessages()'s own notify guard below).
 *
 * setAuthContext()/consumePendingClassifyTarget() are the JS<->native bridge
 * that path needs: OrchieShell.jsx pushes the real token + active sustain
 * whenever either changes (native can't read the WebView's localStorage),
 * and reads back whichever specific item a notification tap was for (native
 * can't itself update React state -- MainActivity just stashes the target,
 * this reads and clears it once).
 *
 * Every message this module ever sees has ALREADY passed the native
 * privacy filter (SmsSenderFilter.isKnownFinancialSender, applied
 * identically to both the backfill and the live-queue path) -- nothing
 * else is ever readable from here.
 *
 * IN-FIELD CLASSIFY NOTIFICATION (JS-side fallback): when a forwarded
 * capture comes back needing a human decision (status="needs_attention" --
 * unmapped or ambiguous, not a source-lookup problem), this module fires a
 * LOCAL notification (@capacitor/local-notifications -- never leaves the
 * device, a different trust class from the SMS-forwarding decision that
 * needed a custom plugin) so the classification happens in the field, in
 * the moment, per Bonnie's explicit design intent -- never deferred to the
 * laptop. Best-effort: a missing/denied notification permission just means
 * no notification fires; the item still lands in Orchie's compose feed
 * exactly as before, so nothing is ever lost, only the nudge is skipped.
 */
import { registerPlugin } from '@capacitor/core';
import { LocalNotifications } from '@capacitor/local-notifications';
import { api, apiBase } from './api';

const SmsCapture = registerPlugin('SmsCapture');

/** Pushes the real, current auth session + active sustain into native
 *  storage so IngestWorker can make a real authenticated capture POST for
 *  a real-time SMS with no JS/webview involved. Best-effort, silent --
 *  call from OrchieShell.jsx whenever the token or selected sustain
 *  changes. A failure here just means the native real-time path stays
 *  inactive until the next successful push; the JS poll/backfill fallback
 *  is unaffected either way. */
export async function setNativeAuthContext(token, sustainId) {
  try {
    await SmsCapture.setAuthContext({ token: token || null, sustainId: sustainId || null, apiBase: apiBase() });
  } catch {
    // native plugin unavailable (web/Tauri) or call failed -- fine, silent.
  }
}

/** Reads (and clears) whatever sustain+message a tap on the native
 *  "Orchie needs a decision" notification deposited, or null if nothing is
 *  pending. Call once on mount and again whenever the app becomes visible
 *  (a notification tap while the app was already running routes through
 *  Android's onNewIntent, not a fresh mount). */
export async function consumePendingClassifyTarget() {
  try {
    const data = await SmsCapture.consumePendingClassifyTarget();
    if (data && data.sustainId && data.messageId) {
      return { sustainId: data.sustainId, messageId: data.messageId };
    }
    return null;
  } catch {
    return null;
  }
}

/** Best-effort -- silently requests the (non-sensitive, on-device-only)
 *  notification permission. Call once, after SMS capture is enabled. */
export async function ensureNotificationPermission() {
  try {
    const status = await LocalNotifications.checkPermissions();
    if (status.display === 'granted') return true;
    const req = await LocalNotifications.requestPermissions();
    return req.display === 'granted';
  } catch {
    return false;
  }
}

let notificationId = 1;

async function notifyNeedsClassification(captureResult) {
  try {
    const status = await LocalNotifications.checkPermissions();
    if (status.display !== 'granted') return; // never force-prompt from here
    const fields = captureResult?.parsed_fields || {};
    const body = (fields.amount != null && fields.counterparty)
      ? `Ksh ${fields.amount} to ${fields.counterparty} — tap to classify`
      : 'a captured transaction needs a quick decision';
    await LocalNotifications.schedule({
      notifications: [{
        id: notificationId++,
        title: 'Orchie needs a decision',
        body,
        schedule: { at: new Date(Date.now() + 300) },
      }],
    });
  } catch (e) {
    console.error('[smsCapture] failed to fire classify notification:', e.message || e);
  }
}

function classifySource(sender) {
  // Purely SENDER-based, never inspects msg.body -- confirmed correct by
  // Bonnie against his real device (1 Aug 2026): every KCB<->M-Pesa-network
  // notification he receives (including ones whose text says "M-PESA")
  // genuinely arrives from the KCB sender id, not MPESA. Classifying by
  // wording instead of sender would have misrouted all of those. The
  // matching server-side split lives in transducer.py's _parse_kcb vs
  // _parse_mpesa (see that module's own header note on the KCB section).
  //
  // Audited again 2 Aug 2026 against a real "source: mpesa" mis-tag Bonnie
  // hit live, on a message his screenshots confirm genuinely arrived from
  // the "KCB" sender: no body-content inference exists anywhere in this
  // function (it never even receives the message body) -- if a real
  // KCB-sender string ever also happened to contain "MPESA" as a raw
  // substring, checking KCB FIRST (reordered here, was MPESA-first) is a
  // harmless, defense-in-depth hardening so that ambiguity can never
  // silently favor "mpesa". The real structural fix for the underlying
  // symptom (a mis-tagged message never being tried against the right
  // parser set) lives server-side: transducer.py's parse_message() is now
  // source-strict -- see its own _PARSERS_BY_SOURCE comment.
  const s = (sender || '').toUpperCase();
  if (s.includes('KCB')) return 'kcb';
  if (s.includes('MPESA')) return 'mpesa';
  return null; // shouldn't happen -- native side already filtered; defense in depth only
}

async function forwardMessages(sustainId, messages) {
  let forwarded = 0;
  for (const msg of messages || []) {
    const sourceId = classifySource(msg.sender);
    if (!sourceId) continue;
    try {
      // POST /api/v1/ingest/capture wraps its real result in the shared
      // {status,data,error,timestamp} envelope (_ok() in ingest.py) --
      // the actual capture outcome (status/is_duplicate/parsed_fields)
      // lives at resp.data, not on resp itself.
      const resp = await api.post('/api/v1/ingest/capture', {
        source_id: sourceId,
        sustain_id: sustainId,
        raw_payload: msg.body,
        captured_at: new Date(msg.timestampMs).toISOString(),
      });
      forwarded += 1;
      const result = resp?.data;
      // Only a genuinely NEW needs_attention capture is worth a nudge --
      // is_duplicate=true means this exact message was already seen
      // (e.g. re-forwarded during a backfill), so notifying again would
      // just spam for something the person has already had a chance to
      // classify (or already did).
      if (result && result.is_duplicate !== true && result.status === 'needs_attention') {
        notifyNeedsClassification(result).catch(() => {});
      }
    } catch (e) {
      // Deliberately don't rethrow -- one failed/malformed message must
      // never block the rest of the batch. Capture is idempotent
      // (dedup_key on the server), so a retry on the next poll/backfill
      // is always safe -- nothing here needs its own retry logic.
      console.error('[smsCapture] failed to forward a message:', e.message || e);
    }
  }
  return forwarded;
}

/** {sms: 'granted'|'denied'|'prompt'|'prompt-with-rationale'} */
export async function checkSmsPermission() {
  const status = await SmsCapture.checkPermissions();
  return status.sms;
}

/** Triggers the real Android system permission dialog. Call only after
 *  showing the in-app rationale (SmsCaptureCard) -- never on cold launch. */
export async function requestSmsPermission() {
  const status = await SmsCapture.requestPermissions();
  return status.sms;
}

/** One-time backfill of existing M-Pesa/KCB messages already in the inbox. */
export async function backfillInbox(sustainId, sinceDays = 90) {
  const { messages } = await SmsCapture.readInbox({ sinceDays });
  return forwardMessages(sustainId, messages);
}

/** Drains whatever the native receiver captured since the last drain. */
export async function drainLiveQueue(sustainId) {
  const { messages } = await SmsCapture.drainQueue();
  if (!messages || messages.length === 0) return 0;
  return forwardMessages(sustainId, messages);
}

let pollHandle = null;

/** Short local poll (cheap -- reads a SharedPreferences queue, no network
 *  call itself) while the app is foregrounded. Call stopLivePolling() on
 *  unmount/backgrounding to avoid leaking the interval. */
export function startLivePolling(sustainId, intervalMs = 20000) {
  stopLivePolling();
  pollHandle = setInterval(() => {
    drainLiveQueue(sustainId).catch(() => {});
  }, intervalMs);
}

export function stopLivePolling() {
  if (pollHandle) {
    clearInterval(pollHandle);
    pollHandle = null;
  }
}
