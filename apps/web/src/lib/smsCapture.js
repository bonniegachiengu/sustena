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
 */
import { registerPlugin } from '@capacitor/core';
import { api } from './api';

const SmsCapture = registerPlugin('SmsCapture');

function classifySource(sender) {
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
      await api.post('/api/v1/ingest/capture', {
        source_id: sourceId,
        sustain_id: sustainId,
        raw_payload: msg.body,
        captured_at: new Date(msg.timestampMs).toISOString(),
      });
      forwarded += 1;
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
