package online.vyybandasky.sustena.smscapture;

import android.content.Context;
import android.content.SharedPreferences;
import java.util.ArrayList;
import java.util.List;

/**
 * Which senders this device will even look at.
 *
 * <p>★★★ The first gate, and the one that matters most for privacy: a text
 * from a sender no thread claims is never read, never queued, never forwarded
 * and never stored. Everything else on a person's phone stays theirs.
 *
 * <p>★★★ It used to be two hard-coded names -- MPESA and KCB -- and that was
 * the ceiling on the whole feature. A household banking anywhere else had no
 * way in at all, because their texts were dropped here before any rule, any
 * training or any Rust could see them. The senders are now declared by the
 * person, pushed down from the app, and kept in SharedPreferences so the
 * BroadcastReceiver can consult them while the app is not running.
 *
 * <p>★★ This mirrors {@code sustena_core::sender}, deliberately and at a real
 * cost. The receiver runs where no Rust does, so one rule lives in two places.
 * Both copies say so, and both do the same thing: case-insensitive substring
 * match against the sender the network reported, never against the body.
 *
 * <p>★ The shipped two are the fallback when nothing has been pushed yet --
 * on a fresh install, before the app has ever run, an M-Pesa text must still
 * be caught.
 */
final class SmsSenderFilter {
    private SmsSenderFilter() {}

    private static final String PREFS = "sustena_sms_threads";
    private static final String KEY_SENDERS = "senders";
    /** What ships, so a fresh install reads money texts before it is configured. */
    private static final String[] BUILT_IN = {"KCB", "MPESA"};

    /** Replace the declared sender list. Pushed from the app; newline-separated. */
    static void setSenders(Context ctx, String newlineSeparated) {
        prefs(ctx).edit().putString(KEY_SENDERS, newlineSeparated == null ? "" : newlineSeparated).apply();
    }

    static boolean isKnownFinancialSender(Context ctx, String sender) {
        if (sender == null) return false;
        String s = sender.trim().toUpperCase();
        if (s.isEmpty()) return false;
        for (String needle : senders(ctx)) {
            if (!needle.isEmpty() && s.contains(needle)) return true;
        }
        return false;
    }

    /**
     * The senders in force.
     *
     * <p>★★ An explicitly EMPTY declaration is honoured as empty rather than
     * falling back: a person who has removed every thread has said something,
     * and quietly restoring the built-ins would read texts he told us not to.
     * Only the absence of any declaration falls back.
     */
    private static List<String> senders(Context ctx) {
        List<String> out = new ArrayList<>();
        String raw = prefs(ctx).getString(KEY_SENDERS, null);
        if (raw == null) {
            for (String b : BUILT_IN) out.add(b);
            return out;
        }
        for (String line : raw.split("\n")) {
            String t = line.trim().toUpperCase();
            if (!t.isEmpty()) out.add(t);
        }
        return out;
    }

    private static SharedPreferences prefs(Context ctx) {
        return ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }
}
