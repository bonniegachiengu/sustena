package online.vyybandasky.sustena.smscapture;

/**
 * The ONE place that decides "is this SMS sender M-Pesa or KCB" -- used by
 * both SmsReceiver (real-time) and SmsCapturePlugin.readInbox() (backfill),
 * so the privacy filter can never drift between the two paths. Every other
 * SMS on the device -- personal texts, OTPs, any other app's messages --
 * must never pass this check.
 *
 * Real Safaricom confirmations arrive from the sender id "MPESA" and KCB's
 * from one containing "KCB" (case varies by device and carrier, hence the
 * uppercase compare). Both were checked against real messages on a real
 * handset.
 *
 * The source is the SENDER, never the wording. Several real KCB messages say
 * M-PESA in their own text, and they are still KCB.
 */
final class SmsSenderFilter {
    private SmsSenderFilter() {}

    static boolean isKnownFinancialSender(String sender) {
        if (sender == null) return false;
        String s = sender.trim().toUpperCase();
        return s.contains("MPESA") || s.contains("KCB");
    }
}
