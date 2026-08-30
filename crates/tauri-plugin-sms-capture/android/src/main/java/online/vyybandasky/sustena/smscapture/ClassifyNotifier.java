package online.vyybandasky.sustena.smscapture;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.os.Build;

/**
 * The prompt that makes capture worth having.
 *
 * <p><b>The gap this closes.</b> {@link SmsReceiver} already fires when the app
 * is closed and already writes the text down. What it did not do was
 * <i>say so</i> — so a captured transaction sat silently in a queue until the
 * app happened to be opened, which turns a real-time reader into a batch
 * importer. Sensing without telling anybody is not a feature; it is a log.
 *
 * <p><b>What it deliberately does not do.</b> It does not open the engine, does
 * not commit anything, and does not touch the network. It cannot: when a text
 * arrives the identity is normally sealed. A notification is a <i>request for a
 * decision</i>, and the decision is made after unlock, by a person, through the
 * same gate every other write goes through. Nothing here shortens that path —
 * it only removes the part where nobody knew there was a decision waiting.
 *
 * <p><b>One notification, replaced rather than stacked.</b> A fixed id means a
 * second transaction updates the prompt instead of adding to a pile. The queue
 * is the queue; the notification is a doorbell, and a doorbell that rings once
 * per caller and keeps ringing is worse than one that says "somebody is here".
 * The count travels in the text so nothing is hidden by that choice.
 */
public final class ClassifyNotifier {

    private static final String CHANNEL = "sustena.classify";
    /** Fixed on purpose — see the class note on replacing rather than stacking. */
    private static final int NOTIFICATION_ID = 4801;

    private static final String PREFS = "sustena_sms_queue";
    private static final String KEY_PENDING = "pending_classify";

    private ClassifyNotifier() {}

    /**
     * Ring for a text that just landed.
     *
     * <p>Best-effort by design and silent on every failure. A receiver has
     * about ten seconds before Android considers it wedged, and the text is
     * <b>already safely queued</b> by the time this runs — so a missing
     * permission, a stripped launch intent or a manufacturer's notification
     * policy must cost the capture nothing. The worst case is the behaviour
     * that existed before this class: the message waits quietly for the app to
     * open.
     */
    static void prompt(Context ctx, String sender, String body, int waiting) {
        try {
            if (!allowed(ctx)) return;

            NotificationManager nm =
                    (NotificationManager) ctx.getSystemService(Context.NOTIFICATION_SERVICE);
            if (nm == null) return;
            ensureChannel(nm);

            String title = waiting > 1
                    ? waiting + " transactions to file"
                    : "A transaction to file";

            Notification n = new Notification.Builder(ctx, CHANNEL)
                    .setContentTitle(title)
                    .setContentText(SmsGlance.line(body))
                    .setSmallIcon(ctx.getApplicationInfo().icon)
                    .setAutoCancel(true)
                    .setContentIntent(openToClassify(ctx))
                    .build();
            nm.notify(NOTIFICATION_ID, n);

            // Remembered here rather than passed through the intent alone,
            // because Android is free to deliver a tap into a process that has
            // been restarted since, and the extras of a recycled PendingIntent
            // are not something to bet a decision on.
            rememberPending(ctx, sender, body);
        } catch (Throwable ignored) {
            // Capture already succeeded. Nothing about telling somebody is
            // worth failing it for.
        }
    }

    /** Clear the prompt once the queue has actually been dealt with. */
    public static void clear(Context ctx) {
        try {
            NotificationManager nm =
                    (NotificationManager) ctx.getSystemService(Context.NOTIFICATION_SERVICE);
            if (nm != null) nm.cancel(NOTIFICATION_ID);
        } catch (Throwable ignored) {
        }
    }

    /**
     * What the tap was about, taken once.
     *
     * <p>Consumed rather than read, so a stale target cannot re-open the same
     * card on a later launch that had nothing to do with it.
     */
    public static synchronized String consumePending(Context ctx) {
        SharedPreferences prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        String pending = prefs.getString(KEY_PENDING, null);
        if (pending != null) prefs.edit().remove(KEY_PENDING).apply();
        return pending;
    }

    private static synchronized void rememberPending(Context ctx, String sender, String body) {
        // The BODY is the identity, because the engine keys an intake on the
        // fact rather than on an id this side could mint (ING-5). Whatever the
        // engine makes of this text on unlock, it will be the same message.
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .edit()
                .putString(KEY_PENDING, body)
                .apply();
    }

    /**
     * Is posting a notification permitted right now?
     *
     * <p>Android 13 made this a runtime permission. Asked for in the app with a
     * reason, like the SMS permissions; checked here so a refusal is quiet
     * rather than an exception in a broadcast receiver.
     */
    private static boolean allowed(Context ctx) {
        if (Build.VERSION.SDK_INT < 33) return true;
        return ctx.checkSelfPermission("android.permission.POST_NOTIFICATIONS")
                == PackageManager.PERMISSION_GRANTED;
    }

    private static void ensureChannel(NotificationManager nm) {
        if (Build.VERSION.SDK_INT < 26) return;
        if (nm.getNotificationChannel(CHANNEL) != null) return;
        NotificationChannel c = new NotificationChannel(
                CHANNEL, "Transactions to file", NotificationManager.IMPORTANCE_DEFAULT);
        c.setDescription(
                "One prompt when a bank or M-Pesa message arrives and needs filing. "
                        + "Nothing is sent anywhere; the message is only read on this phone.");
        c.setShowBadge(true);
        nm.createNotificationChannel(c);
    }

    /**
     * The tap target: this app's own launcher entry, told to go to classify.
     *
     * <p>Resolved from the package manager rather than naming an activity, so
     * the plugin does not have to know what the host app called its main
     * activity — and does not silently break when it is renamed.
     */
    private static PendingIntent openToClassify(Context ctx) {
        Intent launch = ctx.getPackageManager().getLaunchIntentForPackage(ctx.getPackageName());
        if (launch == null) launch = new Intent(Intent.ACTION_MAIN);
        launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        launch.putExtra("sustena_classify", true);

        int flags = PendingIntent.FLAG_UPDATE_CURRENT;
        if (Build.VERSION.SDK_INT >= 23) flags |= PendingIntent.FLAG_IMMUTABLE;
        return PendingIntent.getActivity(ctx, NOTIFICATION_ID, launch, flags);
    }
}
