package online.vyybandasky.sustena;

/**
 * The ONE place that decides "is this SMS sender M-Pesa or KCB" -- used by
 * both SmsReceiver (real-time) and SmsCapturePlugin.readInbox() (backfill),
 * so the privacy filter can never drift between the two paths. Every other
 * SMS on the device -- personal texts, OTPs, any other app's messages --
 * must never pass this check.
 *
 * Real Safaricom M-Pesa confirmations arrive from the sender id "MPESA"
 * (case varies by device/carrier normalization, hence the uppercase
 * comparison). KCB's real sender id is UNVERIFIED here -- same honest
 * disclosure as transducer.py's KCB regexes: "KCB" is the best-effort
 * guess pending Bonnie confirming the exact sender id his phone shows for
 * a real KCB alert. If the real sender id is something else entirely
 * (e.g. a numeric shortcode with no "KCB" substring), this filter --
 * and only this one file -- needs updating.
 */
final class SmsSenderFilter {
    private SmsSenderFilter() {}

    static boolean isKnownFinancialSender(String sender) {
        if (sender == null) return false;
        String s = sender.trim().toUpperCase();
        return s.contains("MPESA") || s.contains("KCB");
    }
}
