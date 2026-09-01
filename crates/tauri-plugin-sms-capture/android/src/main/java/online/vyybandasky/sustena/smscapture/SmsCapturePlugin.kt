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
class SetThreadsArgs {
    /** Sender strings, already flattened from the declared threads. */
    var senders: List<String> = emptyList()
}

@InvokeArg
class ReadInboxArgs {
    /**
     * Where this household's record begins, inclusive, as unix milliseconds.
     *
     * ★★★ The intake boundary, applied at the CONTENT QUERY. A message older
     * than this is never read off the phone at all -- not read, not returned,
     * not captured, not queued. That is the strongest form of the boundary
     * available: the backlog is not filtered after the fact, it never enters.
     *
     * ★★ Inclusive, unlike `sinceMs`. "From this moment" includes this moment,
     * and an exclusive edge would silently drop the very message a person set
     * the cutoff to catch.
     *
     * 0 means no start, which is open -- the behaviour before this existed.
     */
    var startAtMs: Long = 0
    /** 0 or less means the whole inbox. */
    var sinceDays: Int = 0
    /** How many matching messages to skip before collecting. */
    var offset: Int = 0
    /** How many to collect. 0 or less means all of them, which the caller should avoid. */
    var limit: Int = 200
    /**
     * Only messages newer than this, as the device timestamps them.
     *
     * The caller's high-water mark. A repeat read passes the newest it saw last
     * time, so it walks what arrived since rather than the whole inbox again.
     */
    var sinceMs: Long = 0
}

@InvokeArg
class DrainArgs {
    /** How many to take. 0 or less means all of them. */
    var limit: Int = 200
}

/**
 * The bridge. Two ways in, and both apply the same two checks so they can
 * never drift apart:
 *
 *   readInbox   reads texts already on the phone. This is the one that saves
 *               a person from pasting a thousand messages by hand.
 *   drainQueue  hands over what arrived while the app was closed, and clears
 *               what it handed over.
 *
 * BOTH ARE PAGED, and that is the fix for a real failure. A phone with a few
 * thousand texts returned every match in one call, the caller then processed
 * the whole lot before returning, and the app sat frozen with the button still
 * reading "read my texts". Measured on the device that reported it: 6,078
 * texts, of which 2,779 matched. A page at a time keeps every call short and
 * gives the caller somewhere to show progress.
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
        ),
        // ★★★ A SEPARATE alias, deliberately. Reading texts and telling
        //     somebody about one are different asks, and a person who declines
        //     the second should still get a working importer rather than
        //     nothing. Bundling them would make the notification refusal look
        //     like an SMS refusal.
        Permission(strings = ["android.permission.POST_NOTIFICATIONS"], alias = "notify")
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
        var matched = 0      // matching messages seen, including the ones skipped
        var hasMore = false

        val projection = arrayOf(Telephony.Sms.ADDRESS, Telephony.Sms.BODY, Telephony.Sms.DATE)
        // Two floors, and the later one wins: a window the caller asked for,
        // and the high-water mark from the last completed read.
        var floorMs = 0L
        if (args.sinceDays > 0) {
            floorMs = System.currentTimeMillis() - args.sinceDays * 24L * 60L * 60L * 1000L
        }
        if (args.sinceMs > floorMs) {
            // Strictly newer, so the last message read is not offered again.
            floorMs = args.sinceMs + 1
        }
        // ★★★ The household's start, and it is INCLUSIVE. Applied last because
        //     it is the boundary rather than a convenience: a person who says
        //     "begin here" must not have an older floor quietly widened back.
        if (args.startAtMs > 0 && args.startAtMs > floorMs) {
            floorMs = args.startAtMs
        }
        var selection: String? = null
        var selectionArgs: Array<String>? = null
        if (floorMs > 0) {
            selection = Telephony.Sms.DATE + " >= ?"
            selectionArgs = arrayOf(floorMs.toString())
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
                        if (!SmsSenderFilter.isKnownFinancialSender(activity, sender)) {
                            // Counted once, on the first page, so a total is not
                            // multiplied by the number of pages.
                            if (args.offset == 0) filtered++
                            continue // someone else's text. Never leaves this loop.
                        }
                        val body = cursor.getString(bIdx)
                        if (SmsSecretFilter.containsSensitiveSecret(body)) {
                            if (args.offset == 0) secrets++
                            continue // a one-time code. Never returned, whoever sent it.
                        }
                        matched++
                        // Walking past the earlier pages costs a cursor step each.
                        // Cheap next to what the caller does with a message.
                        if (matched <= args.offset) continue
                        if (args.limit > 0 && out.length() >= args.limit) {
                            hasMore = true
                            break
                        }
                        out.put(row(sender, body, cursor.getLong(dIdx)))
                    }
                }
            }
        } catch (e: Exception) {
            invoke.reject("could not read the inbox: ${e.message}")
            return
        }

        val o = batch(out, filtered, secrets)
        o.put("hasMore", hasMore)
        o.put("nextOffset", args.offset + out.length())
        invoke.resolve(o)
    }

    /**
     * Push down which senders this device should look at.
     *
     * ★★★ The receiver runs while the app does not, so it cannot ask the
     * app anything. The list has to be sitting in SharedPreferences before the
     * text arrives, which is why this is pushed rather than pulled.
     *
     * ★★ An empty list is honoured as empty. A person who removed every
     * thread has said something, and quietly restoring the shipped two would
     * read texts he told us not to.
     */
    @Command
    fun setThreads(invoke: Invoke) {
        val args = invoke.parseArgs(SetThreadsArgs::class.java)
        SmsSenderFilter.setSenders(activity, args.senders.joinToString("\n"))
        invoke.resolve()
    }

    @Command
    fun drainQueue(invoke: Invoke) {
        val args = invoke.parseArgs(DrainArgs::class.java)
        // Everything in the queue already passed both checks in the receiver
        // before it was written, so there is nothing to re-check here.
        val result = SmsQueueStore.drain(activity, args.limit)
        val o = batch(result.taken, 0, 0)
        o.put("hasMore", result.remaining > 0)
        o.put("remaining", result.remaining)
        invoke.resolve(o)
    }

    /** How many texts are waiting, without taking any. */
    @Command
    fun queueDepth(invoke: Invoke) {
        val o = JSObject()
        o.put("depth", SmsQueueStore.depth(activity))
        invoke.resolve(o)
    }

    /**
     * What the notification tap was about, taken once.
     *
     * ★★★ Consumed rather than read. A target that survived being acted on
     * would re-open the same card on the next unrelated launch, and a surface
     * that keeps asking about something already dealt with is how a person
     * learns to ignore it.
     *
     * Returns the RAW TEXT, not an id: the engine keys an intake on the fact
     * rather than on an identifier this side could mint (ING-5), so the text is
     * the only handle that means the same thing on both sides of the unlock.
     */
    @Command
    fun consumePendingClassify(invoke: Invoke) {
        val o = JSObject()
        o.put("body", ClassifyNotifier.consumePending(activity))
        invoke.resolve(o)
    }

    /**
     * Take the prompt down.
     *
     * ★★ Called once the queue has actually been swept, not when the app
     * merely opens: a notification cancelled by launching says the work is
     * done when it is not.
     */
    @Command
    fun clearClassifyPrompt(invoke: Invoke) {
        ClassifyNotifier.clear(activity)
        invoke.resolve(JSObject())
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
