package online.vyybandasky.sustena;

import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.os.Build;

import androidx.annotation.NonNull;
import androidx.core.app.NotificationCompat;
import androidx.work.Worker;
import androidx.work.WorkerParameters;

import org.json.JSONObject;

import java.io.BufferedReader;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;
import java.util.TimeZone;

/**
 * Runs the real ingest capture POST natively -- triggered directly by
 * SmsReceiver, deliberately NOT going through the JS/webview layer, so a
 * real bank SMS gets classified-and-notified promptly even when the app
 * process isn't running or is backgrounded with its JS timers frozen
 * (Android is free to suspend a WebView's JS execution the moment the app
 * leaves the foreground -- there is no way to guarantee "immediate" from
 * inside that context alone). This is the ONE place in this app where
 * native code makes a network call -- see SmsReceiver's own header comment
 * for why every OTHER capture path stays JS-only (privacy, not latency);
 * this exists specifically because "detect on arrival, notify almost
 * immediately, regardless of app state" cannot honestly be built any other
 * way on Android.
 *
 * Best-effort, silent on any failure (missing auth context, network error,
 * malformed response): SmsReceiver's own SmsQueueStore enqueue (unchanged,
 * still happens first, always) is the fallback -- the existing JS-side
 * poll/backfill path picks up anything this worker couldn't deliver, the
 * next time the app is opened, and capture is idempotent so there is never
 * a double-apply risk from the two paths overlapping. No retry policy is
 * configured (WorkManager's default is a single attempt) -- a stale retry
 * long after the JS path already handled the same message would just be a
 * harmless is_duplicate=true no-op, not worth the battery.
 */
public class IngestWorker extends Worker {
    private static final String CHANNEL_ID = "sustena_classify";
    private static final String CHANNEL_NAME = "Orchie classify";

    public static final String KEY_SENDER = "sender";
    public static final String KEY_BODY = "body";
    public static final String KEY_TIMESTAMP_MS = "timestampMs";

    public IngestWorker(@NonNull Context context, @NonNull WorkerParameters params) {
        super(context, params);
    }

    @NonNull
    @Override
    public Result doWork() {
        Context ctx = getApplicationContext();
        String sender = getInputData().getString(KEY_SENDER);
        String body = getInputData().getString(KEY_BODY);
        long timestampMs = getInputData().getLong(KEY_TIMESTAMP_MS, System.currentTimeMillis());

        String token = SmsAuthStore.getToken(ctx);
        String sustainId = SmsAuthStore.getSustainId(ctx);
        String apiBase = SmsAuthStore.getApiBase(ctx);
        if (token == null || sustainId == null || apiBase == null || sender == null || body == null) {
            // No auth context cached yet (never signed in on this device
            // since install, or JS hasn't pushed it since a logout) --
            // nothing to do; SmsQueueStore already has the message queued
            // for whenever the app is next opened.
            return Result.success();
        }

        String sourceId = classifySource(sender);
        if (sourceId == null) return Result.success(); // not a recognised financial sender

        try {
            JSONObject reqBody = new JSONObject();
            reqBody.put("source_id", sourceId);
            reqBody.put("sustain_id", sustainId);
            reqBody.put("raw_payload", body);
            reqBody.put("captured_at", isoTimestamp(timestampMs));

            JSONObject envelope = postJson(apiBase + "/api/v1/ingest/capture", token, reqBody);
            if (envelope == null) return Result.success();
            JSONObject data = envelope.optJSONObject("data");
            if (data == null) return Result.success();

            boolean isDuplicate = data.optBoolean("is_duplicate", false);
            String status = data.optString("status", "");
            if (!isDuplicate && "needs_attention".equals(status)) {
                showClassifyNotification(ctx, sustainId, data);
            }
        } catch (Exception e) {
            // Silent, best-effort -- see class docstring. A BroadcastReceiver
            // -triggered background job must never crash over a network hiccup.
        }
        return Result.success();
    }

    /** Mirrors smsCapture.js's classifySource() -- purely sender-based,
     *  never inspects message content. Kept in sync by hand (two different
     *  runtimes, no shared code path between native Java and the webview's
     *  JS), same discipline SmsSecretFilter's own header comment documents
     *  for its own JS-side counterpart. */
    private static String classifySource(String sender) {
        String s = sender == null ? "" : sender.toUpperCase();
        if (s.contains("MPESA")) return "mpesa";
        if (s.contains("KCB")) return "kcb";
        return null;
    }

    /** minSdk is 24 -- java.time.Instant needs API 26+ (or desugaring, not
     *  configured in this project's build). SimpleDateFormat has been
     *  available since API 1, so this is the safe choice here. */
    private static String isoTimestamp(long epochMs) {
        SimpleDateFormat fmt = new SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US);
        fmt.setTimeZone(TimeZone.getTimeZone("UTC"));
        return fmt.format(new Date(epochMs));
    }

    private JSONObject postJson(String urlStr, String token, JSONObject body) throws Exception {
        URL url = new URL(urlStr);
        HttpURLConnection conn = (HttpURLConnection) url.openConnection();
        try {
            conn.setRequestMethod("POST");
            conn.setRequestProperty("Content-Type", "application/json");
            conn.setRequestProperty("Authorization", "Bearer " + token);
            conn.setDoOutput(true);
            conn.setConnectTimeout(10000);
            conn.setReadTimeout(10000);
            try (OutputStream os = conn.getOutputStream()) {
                os.write(body.toString().getBytes(StandardCharsets.UTF_8));
            }
            int code = conn.getResponseCode();
            InputStream stream = (code >= 200 && code < 300) ? conn.getInputStream() : conn.getErrorStream();
            if (stream == null || code < 200 || code >= 300) return null;
            StringBuilder sb = new StringBuilder();
            try (BufferedReader reader = new BufferedReader(new InputStreamReader(stream, StandardCharsets.UTF_8))) {
                String line;
                while ((line = reader.readLine()) != null) sb.append(line);
            }
            return new JSONObject(sb.toString());
        } finally {
            conn.disconnect();
        }
    }

    /** Real, human-legible, immediate -- carries the same amount/merchant
     *  detail the phone-first classify card itself shows (Ksh X to Y),
     *  recovered server-side by the transducer, not guessed natively.
     *  Tapping deep-links into MainActivity with the target sustain +
     *  message so Orchie opens straight to that item's classify/confirm
     *  card (see MainActivity.capturePendingClassifyIntent +
     *  SmsCapturePlugin.consumePendingClassifyTarget). */
    private void showClassifyNotification(Context ctx, String sustainId, JSONObject data) {
        NotificationManager nm = (NotificationManager) ctx.getSystemService(Context.NOTIFICATION_SERVICE);
        if (nm == null) return;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            NotificationChannel channel = new NotificationChannel(CHANNEL_ID, CHANNEL_NAME, NotificationManager.IMPORTANCE_HIGH);
            nm.createNotificationChannel(channel); // idempotent -- safe on every call
        }

        String messageId = data.optString("message_id", null);
        JSONObject parsedFields = data.optJSONObject("parsed_fields");
        String contentText;
        if (parsedFields != null && parsedFields.has("amount") && parsedFields.has("counterparty")) {
            contentText = "Ksh " + parsedFields.optString("amount") + " to " + parsedFields.optString("counterparty") + " — tap to classify";
        } else {
            contentText = "a captured transaction needs a quick decision";
        }

        Intent intent = new Intent(ctx, MainActivity.class);
        intent.setFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        intent.putExtra("sustena_classify_sustain_id", sustainId);
        intent.putExtra("sustena_classify_message_id", messageId);

        int requestCode = messageId != null ? messageId.hashCode() : 0;
        PendingIntent pendingIntent = PendingIntent.getActivity(
            ctx, requestCode, intent, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);

        // A generic system icon, not app-branded -- real Sustena-branded
        // notification icon assets don't exist yet (same disclosed gap as
        // the app/launcher icons from the earlier Android/Tauri packaging
        // sessions). Swap for a real one-color notification icon drawable
        // when brand assets exist; not blocking for this feature.
        NotificationCompat.Builder builder = new NotificationCompat.Builder(ctx, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("Orchie needs a decision")
            .setContentText(contentText)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setAutoCancel(true)
            .setContentIntent(pendingIntent);

        int notificationId = messageId != null ? messageId.hashCode() : (int) System.currentTimeMillis();
        nm.notify(notificationId, builder.build());
    }
}
