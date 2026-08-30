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

    /**
     * Takes at most {@code limit} items and leaves the rest.
     *
     * Bounded on purpose. Draining everything in one call is what froze the
     * app on unlock when a large batch had built up: the caller then had to
     * process the whole lot before it could return. The caller loops instead,
     * and the app stays responsive between batches.
     *
     * A limit of 0 or less means take everything, which is what the tests use.
     */
    static synchronized DrainResult drain(Context ctx, int limit) {
        SharedPreferences prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        JSONArray all = readArray(prefs);
        if (limit <= 0 || all.length() <= limit) {
            prefs.edit().remove(KEY).apply();
            return new DrainResult(all, 0);
        }
        JSONArray taken = new JSONArray();
        JSONArray left = new JSONArray();
        for (int i = 0; i < all.length(); i++) {
            Object item = all.opt(i);
            if (item == null) continue;
            if (i < limit) taken.put(item); else left.put(item);
        }
        prefs.edit().putString(KEY, left.toString()).apply();
        return new DrainResult(taken, left.length());
    }

    /** What was taken, and how much is still waiting. */
    static final class DrainResult {
        final JSONArray taken;
        final int remaining;
        DrainResult(JSONArray taken, int remaining) {
            this.taken = taken;
            this.remaining = remaining;
        }
    }

    /** How many are waiting, without taking any. The queue-depth reading. */
    static synchronized int depth(Context ctx) {
        return readArray(ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)).length();
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
