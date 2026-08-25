package online.vyybandasky.sustena.smscapture;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.provider.Telephony;
import android.telephony.SmsMessage;

/**
 * Fires on every incoming text, registered in the manifest so it still fires
 * when the app is not running. That is the normal case: a text arrives during
 * the day and the app is opened hours later.
 *
 * TWO CHECKS, BOTH MUST PASS. The sender must be M-Pesa or KCB, and the body
 * must not look like a one-time code. The second check is the one that matters:
 * an OTP for a bank action arrives from the SAME sender id as a real
 * confirmation, so the sender alone can never tell them apart.
 *
 * Android hands every text to every registered receiver and there is no way to
 * filter before receiving. So a message that fails either check is read here
 * and dropped here. It is never queued, never written down, never logged.
 *
 * THIS RECEIVER DOES NOTHING ELSE. It does not open a socket and it does not
 * touch the engine. A receiver has about ten seconds before Android considers
 * it wedged; one small write fits comfortably and a network call does not. It
 * also could not write to the engine if it wanted to, because the identity is
 * usually locked when a text arrives. Capturing costs nothing and needs nobody.
 * Applying needs a key, and happens later, when the app is open and unlocked.
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

        // A long text arrives split across several parts. The body is the parts
        // joined; the sender is the same on all of them.
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
            return; // someone else's text. Dropped here.
        }

        String bodyText = body.toString();
        if (SmsSecretFilter.containsSensitiveSecret(bodyText)) {
            return; // a one-time code. Dropped here, whoever sent it.
        }

        SmsQueueStore.enqueue(context, sender, bodyText, System.currentTimeMillis());
    }
}
