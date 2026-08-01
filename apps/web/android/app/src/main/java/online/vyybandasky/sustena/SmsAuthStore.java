package online.vyybandasky.sustena;

import android.content.Context;
import android.content.SharedPreferences;

/**
 * Minimal native-side mirror of the auth session + active sustain selection,
 * so IngestWorker (triggered directly by SmsReceiver, possibly with no
 * JS/webview alive at all) can make a real authenticated capture POST
 * without waiting for the app to be open. JS pushes fresh values here via
 * SmsCapturePlugin.setAuthContext() -- on login, and whenever the selected
 * sustain changes -- see OrchieShell.jsx's own effect for when. This store
 * is a CACHE, not a second source of truth: the real session still lives in
 * the WebView's localStorage; this is only what native code can see without
 * a JS runtime. A stale/missing entry here just means IngestWorker skips
 * (see its own docstring) -- the existing JS-side poll/backfill path is the
 * fallback, unaffected by anything in this class.
 *
 * Also holds the "pending classify target" a notification tap deposits
 * (MainActivity.capturePendingClassifyIntent) for JS to consume exactly
 * once via SmsCapturePlugin.consumePendingClassifyTarget().
 */
final class SmsAuthStore {
    private static final String PREFS = "sustena_auth_context";
    private static final String KEY_TOKEN = "token";
    private static final String KEY_SUSTAIN_ID = "sustain_id";
    private static final String KEY_API_BASE = "api_base";

    private static final String PENDING_PREFS = "sustena_pending_classify";
    private static final String KEY_PENDING_SUSTAIN = "sustain_id";
    private static final String KEY_PENDING_MESSAGE = "message_id";

    private SmsAuthStore() {}

    static void set(Context ctx, String token, String sustainId, String apiBase) {
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
            .putString(KEY_TOKEN, token)
            .putString(KEY_SUSTAIN_ID, sustainId)
            .putString(KEY_API_BASE, apiBase)
            .apply();
    }

    static String getToken(Context ctx) {
        return ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY_TOKEN, null);
    }

    static String getSustainId(Context ctx) {
        return ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY_SUSTAIN_ID, null);
    }

    static String getApiBase(Context ctx) {
        return ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY_API_BASE, null);
    }

    // ── Pending classify target (notification tap -> app open) ─────────────

    static void setPendingClassifyTarget(Context ctx, String sustainId, String messageId) {
        ctx.getSharedPreferences(PENDING_PREFS, Context.MODE_PRIVATE).edit()
            .putString(KEY_PENDING_SUSTAIN, sustainId)
            .putString(KEY_PENDING_MESSAGE, messageId)
            .apply();
    }

    /** Returns {sustainId, messageId} and clears it -- consumed exactly
     *  once, so a later resume/mount doesn't re-open the same card. */
    static String[] consumePendingClassifyTarget(Context ctx) {
        SharedPreferences prefs = ctx.getSharedPreferences(PENDING_PREFS, Context.MODE_PRIVATE);
        String sustainId = prefs.getString(KEY_PENDING_SUSTAIN, null);
        String messageId = prefs.getString(KEY_PENDING_MESSAGE, null);
        if (sustainId == null || messageId == null) return null;
        prefs.edit().clear().apply();
        return new String[]{sustainId, messageId};
    }
}
