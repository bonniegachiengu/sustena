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
 * HONEST SCOPE: "real time" here means captured instantly (nothing is
 * ever missed, regardless of whether the app is running when the SMS
 * arrives) but DELIVERED to the backend on the next short poll while the
 * app is foregrounded, or on resume if it wasn't. This is not always-
 * instant-regardless-of-app-state -- a genuinely instant background push
 * would need a reliable background-work scheduler (WorkManager) doing the
 * networking from native code, which is real, disclosed, later work, not
 * built in this first cut.
 *
 * Every message this module ever sees has ALREADY passed the native
 * privacy filter (SmsSenderFilter.isKnownFinancialSender, applied
 * identically to both the backfill and the live-queue path) -- nothing
 * else is ever readable from here.
 *
 * IN-FIELD CLASSIFY NOTIFICATION: when a forwarded capture comes back
 * needing a human decision (status="needs_attention" -- unmapped or
 * ambiguous, not a source-lookup problem), this module fires a LOCAL
 * notification (@capacitor/local-notifications -- never leaves the device,
 * a different trust class from the SMS-forwarding decision that needed a
 * custom plugin) so the classification happens in the field, in the
 * moment, per Bonnie's explicit design intent -- never deferred to the
 * laptop. Best-effort: a missing/denied notification permission just means
 * no notification fires; the item still lands in Orchie's compose feed
 * exactly as before, so nothing is ever lost, only the nudge is skipped.
 */
import { registerPlugin } from '@capacitor/core';
import { LocalNotifications } from '@capacitor/local-notifications';
import { api } from './api';

const SmsCapture = registerPlugin('SmsCapture');

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
  const s = (sender || '').toUpperCase();
  if (s.includes('MPESA')) return 'mpesa';
  if (s.includes('KCB')) return 'kcb';
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
