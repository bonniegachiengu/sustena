package online.vyybandasky.sustena.smscapture

import android.Manifest
import android.app.Activity
import android.provider.Telephony
import app.tauri.PermissionState
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import app.tauri.plugin.Invoke
import org.json.JSONArray

@InvokeArg
class ReadInboxArgs {
    /** 0 or less means the whole inbox. */
    var sinceDays: Int = 0
}

/**
 * The bridge. Two ways in, and both apply the same two checks so they can
 * never drift apart:
 *
 *   readInbox   reads texts already on the phone. This is the one that saves
 *               a person from pasting a thousand messages by hand.
 *   drainQueue  hands over what arrived while the app was closed, and clears
 *               it, so a message is handed over once.
 *
 * Nothing here writes to the engine or opens a socket. It returns text to the
 * host, and the host captures it through the same path a pasted message takes.
 *
 * checkPermissions and requestPermissions come from the base class, driven by
 * the annotation below.
 */
@TauriPlugin(
    permissions = [
        Permission(
            strings = [Manifest.permission.READ_SMS, Manifest.permission.RECEIVE_SMS],
            alias = "sms"
        )
    ]
)
class SmsCapturePlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun readInbox(invoke: Invoke) {
        if (getPermissionState("sms") != PermissionState.GRANTED) {
            invoke.reject("permission to read texts has not been granted")
            return
        }
        val args = invoke.parseArgs(ReadInboxArgs::class.java)

        val out = JSONArray()
        var filtered = 0
        var secrets = 0

        val projection = arrayOf(Telephony.Sms.ADDRESS, Telephony.Sms.BODY, Telephony.Sms.DATE)
        var selection: String? = null
        var selectionArgs: Array<String>? = null
        if (args.sinceDays > 0) {
            val sinceMs = System.currentTimeMillis() - args.sinceDays * 24L * 60L * 60L * 1000L
            selection = Telephony.Sms.DATE + " >= ?"
            selectionArgs = arrayOf(sinceMs.toString())
        }

        try {
            activity.contentResolver.query(
                Telephony.Sms.Inbox.CONTENT_URI,
                projection,
                selection,
                selectionArgs,
                Telephony.Sms.DATE + " DESC"
            ).use { cursor ->
                if (cursor != null) {
                    val aIdx = cursor.getColumnIndexOrThrow(Telephony.Sms.ADDRESS)
                    val bIdx = cursor.getColumnIndexOrThrow(Telephony.Sms.BODY)
                    val dIdx = cursor.getColumnIndexOrThrow(Telephony.Sms.DATE)
                    while (cursor.moveToNext()) {
                        val sender = cursor.getString(aIdx)
                        if (!SmsSenderFilter.isKnownFinancialSender(sender)) {
                            filtered++
                            continue // someone else's text. Never leaves this loop.
                        }
                        val body = cursor.getString(bIdx)
                        if (SmsSecretFilter.containsSensitiveSecret(body)) {
                            secrets++
                            continue // a one-time code. Never returned, whoever sent it.
                        }
                        out.put(row(sender, body, cursor.getLong(dIdx)))
                    }
                }
            }
        } catch (e: Exception) {
            invoke.reject("could not read the inbox: ${e.message}")
            return
        }

        invoke.resolve(batch(out, filtered, secrets))
    }

    @Command
    fun drainQueue(invoke: Invoke) {
        // Everything in the queue already passed both checks in the receiver
        // before it was written, so there is nothing to re-check here.
        val queued = SmsQueueStore.drain(activity)
        invoke.resolve(batch(queued, 0, 0))
    }

    private fun row(sender: String, body: String, ts: Long) =
        org.json.JSONObject().apply {
            put("sender", sender)
            put("body", body)
            put("timestampMs", ts)
        }

    private fun batch(messages: JSONArray, filtered: Int, secrets: Int): JSObject {
        val o = JSObject()
        o.put("messages", messages)
        o.put("filteredOut", filtered)
        o.put("secretsRefused", secrets)
        return o
    }
}
