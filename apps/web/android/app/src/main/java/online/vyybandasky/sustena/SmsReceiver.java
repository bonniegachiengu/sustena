package online.vyybandasky.sustena;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.provider.Telephony;
import android.telephony.SmsMessage;

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
 * Deliberately does NOT talk to the network -- see SmsQueueStore's own
 * header comment for why (a BroadcastReceiver has a short, ~10s execution
 * window before Android may treat it as non-responsive; a single
 * SharedPreferences write comfortably fits that, a real-world mobile HTTP
 * POST is a genuine risk of exceeding it). smsCapture.js (the JS side)
 * drains the queue on a short poll while the app is foregrounded and once
 * on resume -- see that module's own header comment for the honest
 * "near-real-time, not always-instant-regardless-of-app-state" scope this
 * implies.
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
        if (SmsSecretFilter.containsSensitiveSecret(bodyText)) {
            return; // OTP/verification code -- discarded here, NEVER queued, regardless of sender
        }

        SmsQueueStore.enqueue(context, sender, bodyText, System.currentTimeMillis());
    }
}
