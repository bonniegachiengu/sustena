package online.vyybandasky.sustena.smscapture;

import android.content.Context;
import android.content.SharedPreferences;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/**
 * A tiny persisted queue for SMS captured by SmsReceiver in real time.
 * Exists specifically so a message is never lost even if no JS listener is
 * currently draining it (app backgrounded or not running when the SMS
 * arrives) -- SmsReceiver can only guarantee "captured", not "delivered",
 * within its own short execution window (see SmsReceiver's header comment
 * for why it never talks to the network directly). SmsCapturePlugin's
 * drainQueue() is the only reader; smsCapture.js polls it while the app is
 * foregrounded and once on resume.
 */
final class SmsQueueStore {
    private static final String PREFS = "sustena_sms_queue";
    private static final String KEY = "queue";

    private SmsQueueStore() {}

    static synchronized void enqueue(Context ctx, String sender, String body, long timestampMs) {
        SharedPreferences prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        JSONArray arr = readArray(prefs);
        JSONObject item = new JSONObject();
        try {
            item.put("sender", sender);
            item.put("body", body);
            item.put("timestampMs", timestampMs);
        } catch (JSONException e) {
            return; // malformed -- drop rather than crash the receiver
        }
        arr.put(item);
        prefs.edit().putString(KEY, arr.toString()).apply();
    }

    static synchronized JSONArray drain(Context ctx) {
        SharedPreferences prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        JSONArray arr = readArray(prefs);
        prefs.edit().remove(KEY).apply();
        return arr;
    }

    private static JSONArray readArray(SharedPreferences prefs) {
        String raw = prefs.getString(KEY, "[]");
        try {
            return new JSONArray(raw);
        } catch (JSONException e) {
            return new JSONArray();
        }
    }
}
