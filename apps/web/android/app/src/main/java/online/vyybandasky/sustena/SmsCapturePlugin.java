package online.vyybandasky.sustena;

import android.Manifest;
import android.content.ContentResolver;
import android.database.Cursor;
import android.net.Uri;
import android.provider.Telephony;

import com.getcapacitor.JSArray;
import com.getcapacitor.JSObject;
import com.getcapacitor.PermissionState;
import com.getcapacitor.Plugin;
import com.getcapacitor.PluginCall;
import com.getcapacitor.PluginMethod;
import com.getcapacitor.annotation.CapacitorPlugin;
import com.getcapacitor.annotation.Permission;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/**
 * Native bridge for the SMS auto-reader (M-Pesa + KCB ingest, 1 Aug 2026).
 *
 * checkPermissions()/requestPermissions() need no custom code here --
 * Capacitor's own Plugin base class provides generic implementations
 * driven entirely by the @Permission declaration below; JS calls
 * SmsCapture.checkPermissions()/requestPermissions() and gets back
 * {sms: "granted"|"denied"|"prompt"|"prompt-with-rationale"}.
 *
 * Two read paths, both applying the IDENTICAL privacy filter
 * (SmsSenderFilter.isKnownFinancialSender) so it can never drift:
 *   readInbox()  -- one-time backfill, queries the SMS content provider
 *                    directly for existing messages.
 *   drainQueue() -- returns (and clears) whatever SmsReceiver captured in
 *                    real time since the last drain.
 */
@CapacitorPlugin(
    name = "SmsCapture",
    permissions = {
        @Permission(alias = "sms", strings = { Manifest.permission.READ_SMS, Manifest.permission.RECEIVE_SMS })
    }
)
public class SmsCapturePlugin extends Plugin {

    @PluginMethod
    public void readInbox(PluginCall call) {
        if (getPermissionState("sms") != PermissionState.GRANTED) {
            call.reject("SMS permission not granted");
            return;
        }

        int sinceDays = call.getInt("sinceDays", 90);
        long sinceMs = System.currentTimeMillis() - (sinceDays * 24L * 60L * 60L * 1000L);

        JSArray results = new JSArray();
        ContentResolver resolver = getContext().getContentResolver();
        Uri inbox = Telephony.Sms.Inbox.CONTENT_URI;
        String[] projection = { Telephony.Sms.ADDRESS, Telephony.Sms.BODY, Telephony.Sms.DATE };
        String selection = Telephony.Sms.DATE + " >= ?";
        String[] selectionArgs = { String.valueOf(sinceMs) };
        String sortOrder = Telephony.Sms.DATE + " DESC";

        try (Cursor cursor = resolver.query(inbox, projection, selection, selectionArgs, sortOrder)) {
            if (cursor != null) {
                int addressIdx = cursor.getColumnIndexOrThrow(Telephony.Sms.ADDRESS);
                int bodyIdx = cursor.getColumnIndexOrThrow(Telephony.Sms.BODY);
                int dateIdx = cursor.getColumnIndexOrThrow(Telephony.Sms.DATE);
                while (cursor.moveToNext()) {
                    String sender = cursor.getString(addressIdx);
                    if (!SmsSenderFilter.isKnownFinancialSender(sender)) {
                        continue; // not M-Pesa or KCB -- never returned to JS
                    }
                    JSObject item = new JSObject();
                    item.put("sender", sender);
                    item.put("body", cursor.getString(bodyIdx));
                    item.put("timestampMs", cursor.getLong(dateIdx));
                    results.put(item);
                }
            }
        } catch (Exception e) {
            call.reject("Failed to read SMS inbox: " + e.getMessage());
            return;
        }

        JSObject ret = new JSObject();
        ret.put("messages", results);
        call.resolve(ret);
    }

    @PluginMethod
    public void drainQueue(PluginCall call) {
        JSONArray queued = SmsQueueStore.drain(getContext());
        JSArray results = new JSArray();
        for (int i = 0; i < queued.length(); i++) {
            try {
                JSONObject item = queued.getJSONObject(i);
                JSObject out = new JSObject();
                out.put("sender", item.getString("sender"));
                out.put("body", item.getString("body"));
                out.put("timestampMs", item.getLong("timestampMs"));
                results.put(out);
            } catch (JSONException e) {
                // one malformed queued item -- skip it, don't fail the whole drain
            }
        }
        JSObject ret = new JSObject();
        ret.put("messages", results);
        call.resolve(ret);
    }
}
