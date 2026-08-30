package online.vyybandasky.sustena.smscapture;

import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * The one line a notification can say about a text, read on the device.
 *
 * <p><b>THIS IS NOT THE PARSER, AND IT MUST NEVER BECOME ONE.</b> The parser is
 * the transducer in the engine: it owns the declared rule set, the source-strict
 * routing, the secret pre-gate and every shape this project has learned from
 * real messages. Nothing here writes state, decides a source, chooses an
 * operator, or reaches a balance. Everything it produces is <i>display only</i>
 * and is thrown away the moment the engine has read the same text properly.
 *
 * <p>It exists because of a hard constraint rather than a preference. When a
 * text arrives the app is usually closed and the identity is <b>sealed</b> —
 * the engine cannot be opened without the passphrase, so the authoritative
 * parse genuinely cannot run yet. The choice is therefore not "parse here or
 * parse in the engine"; it is "put a figure on the notification, or show a
 * notification that says nothing".
 *
 * <p>A notification reading <i>"New M-Pesa message"</i> is nearly useless: it
 * cannot be triaged, so every one of them costs a full app open to find out
 * whether it mattered. One reading <i>"Ksh 680 to NAIVAS"</i> can be judged in
 * the lock screen. That is the whole difference between a prompt and a
 * nuisance.
 *
 * <p><b>Its failure mode is silence, not error.</b> Every field is optional and
 * anything it cannot read is simply absent from the notification text — it
 * never guesses, never rounds, and never substitutes. A glance that reads
 * nothing produces a generic prompt, which is exactly what the honest answer
 * looks like.
 */
final class SmsGlance {

    /**
     * A currency-prefixed figure. Deliberately narrower than a bare digit
     * match: a raw SMS is full of digits from dates, reference codes and phone
     * fragments, and none of those is the amount.
     */
    private static final Pattern AMOUNT =
            Pattern.compile("(?i)\\b(?:ksh|kes)\\s*\\.?\\s*([0-9][0-9,]*(?:\\.[0-9]{1,2})?)");

    /** The counterparty, in the three shapes that carry one. */
    private static final Pattern[] WHO = {
            Pattern.compile("(?i)\\bpaid to\\s+([A-Za-z0-9&'.\\- ]{2,40}?)\\s+(?:on|for|\\d)"),
            Pattern.compile("(?i)\\bsent to\\s+([A-Za-z0-9&'.\\- ]{2,40}?)\\s+(?:on|for|\\d)"),
            Pattern.compile("(?i)\\bfrom\\s+([A-Za-z0-9&'.\\- ]{2,40}?)\\s+(?:on|\\d)"),
    };

    private SmsGlance() {}

    /** The amount as it was written, or null. Never reformatted. */
    static String amount(String body) {
        if (body == null) return null;
        Matcher m = AMOUNT.matcher(body);
        return m.find() ? m.group(1) : null;
    }

    /** Who the money moved to or from, or null. */
    static String counterparty(String body) {
        if (body == null) return null;
        for (Pattern p : WHO) {
            Matcher m = p.matcher(body);
            if (m.find()) {
                String who = m.group(1).trim();
                if (!who.isEmpty()) return who;
            }
        }
        return null;
    }

    /**
     * One short line for the notification body.
     *
     * <p>Degrades in steps rather than all at once: amount and counterparty,
     * then amount alone, then a plain prompt. Each step is still actionable —
     * the last one says a transaction arrived and needs filing, which is true
     * and is the minimum worth waking somebody for.
     */
    static String line(String body) {
        String amount = amount(body);
        String who = counterparty(body);
        if (amount != null && who != null) return "Ksh " + amount + " — " + who;
        if (amount != null) return "Ksh " + amount;
        if (who != null) return who;
        return "A transaction arrived and needs filing";
    }
}
