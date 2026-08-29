package online.vyybandasky.sustena;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.provider.Telephony;
import android.telephony.SmsMessage;

import androidx.work.Data;
import androidx.work.OneTimeWorkRequest;
import androidx.work.OutOfQuotaPolicy;
import androidx.work.WorkManager;

/**
 * Fires on every incoming SMS (android.provider.Telephony.SMS_RECEIVED_ACTION),
 * registered statically in AndroidManifest.xml so a real financial SMS is
 * captured even if the app isn't currently running.
 *
 * STRICT PRIVACY FILTER, two checks, BOTH must pass: the sender must match
 * SmsSenderFilter.isKnownFinancialSender() AND the body must NOT match
 * SmsSecretFilter.containsSensitiveSecret() (CRITICAL -- a real OTP for a
 * KCB/M-Pesa action arrives from the exact same sender id as a real
 * transaction confirmation, so the sender check alone cannot catch it).
 * Android delivers the full SMS_RECEIVED broadcast to every registered
 * receiver regardless of sender -- there is no way to filter before
 * receiving it -- so every message that fails either check is read from
 * the intent here and then immediately discarded: never queued, never
 * logged, never touched again.
 *
 * This receiver ITSELF still never talks to the network (a BroadcastReceiver
 * has a short, ~10s execution window before Android may treat it as
 * non-responsive -- a single SharedPreferences write and a WorkManager
 * enqueue both comfortably fit that, a real-world mobile HTTP POST made
 * directly here would not). Two things happen for a message that passes
 * both filters:
 *   1. SmsQueueStore.enqueue() -- unchanged, the JS-side fallback path
 *      (smsCapture.js polls this while the app is foregrounded / on resume).
 *   2. A WorkManager job (IngestWorker) is enqueued to do the REAL capture
 *      POST + classify notification natively, near-immediately, regardless
 *      of whether the app process is even running -- see IngestWorker's own
 *      header comment for why this exists as a second path rather than
 *      relying on (1) alone: "immediate, detect-on-arrival" cannot honestly
 *      be delivered by a webview JS timer Android is free to suspend.
 *      OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST means "run as
 *      soon as possible, and if there's no expedited-job quota available
 *      right now, just run as normal background work instead of failing" --
 *      no extra foreground-service permission needed.
 */
public class SmsReceiver extends BroadcastReceiver {

    @Override
    public void onReceive(Context context, Intent intent) {
        if (!Telephony.Sms.Intents.SMS_RECEIVED_ACTION.equals(intent.getAction())) {
            return;
        }
        SmsMessage[] parts = Telephony.Sms.Intents.getMessagesFromIntent(intent);
        if (parts == null || parts.length == 0) {
            return;
        }

        // A single logical SMS can arrive split across multiple PDU parts
        // (long messages) -- concatenate the body; the originating address
        // is the same across every part of one message.
        StringBuilder body = new StringBuilder();
        String sender = null;
        for (SmsMessage part : parts) {
            if (part == null) continue;
            if (sender == null) sender = part.getOriginatingAddress();
            body.append(part.getMessageBody());
        }
        if (sender == null || body.length() == 0) {
            return;
        }

        if (!SmsSenderFilter.isKnownFinancialSender(sender)) {
            return; // not M-Pesa or KCB -- discarded here, never queued
        }

        String bodyText = body.toString();
        // TWO detectors, consulted with OR (SmsShapeFilter). The vocabulary
        // knows phrasings; the shape reads structure and no words at all, so a
        // bank inventing new wording -- or writing in Swahili -- defeats one and
        // not the other. Either refusing is a refusal, because a false positive
        // costs one capture re-entered by hand and a false negative puts a live
        // credential on the wire.
        if (SmsShapeFilter.mustNotLeaveTheDevice(bodyText)) {
            return; // credential -- discarded here, NEVER queued, regardless of sender
        }

        long timestampMs = System.currentTimeMillis();
        SmsQueueStore.enqueue(context, sender, bodyText, timestampMs);

        Data inputData = new Data.Builder()
            .putString(IngestWorker.KEY_SENDER, sender)
            .putString(IngestWorker.KEY_BODY, bodyText)
            .putLong(IngestWorker.KEY_TIMESTAMP_MS, timestampMs)
            .build();
        OneTimeWorkRequest request = new OneTimeWorkRequest.Builder(IngestWorker.class)
            .setInputData(inputData)
            .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
            .build();
        WorkManager.getInstance(context).enqueue(request);
    }
}
