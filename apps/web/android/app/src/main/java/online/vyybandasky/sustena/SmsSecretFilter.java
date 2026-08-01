package online.vyybandasky.sustena;

import java.util.regex.Pattern;

/**
 * CRITICAL, security-relevant. The PRIMARY defense against ever sending an
 * OTP / verification-code SMS off this device -- the server (transducer.py's
 * contains_sensitive_secret()) applies the identical check as defense in
 * depth, in case one ever reaches it anyway, but this is the check that
 * actually matters: a real OTP for a KCB or M-Pesa action arrives from the
 * exact same sender id as a real transaction confirmation ("MPESA"/"KCB"),
 * so SmsSenderFilter alone (sender-based) cannot distinguish them. This
 * filter is content-based and applied IN ADDITION to SmsSenderFilter, in
 * both SmsReceiver (real-time) and SmsCapturePlugin.readInbox() (backfill)
 * -- a message must pass BOTH checks before it is ever queued or returned
 * to JS.
 *
 * The keyword set mirrors transducer.py's contains_sensitive_secret()
 * deliberately -- kept in sync by hand (two different runtimes, no shared
 * code path), narrow and high-confidence vocabulary that essentially never
 * appears in a legitimate transaction confirmation.
 */
final class SmsSecretFilter {
    private SmsSecretFilter() {}

    private static final Pattern[] SENSITIVE_PATTERNS = {
        Pattern.compile("\\bOTP\\b", Pattern.CASE_INSENSITIVE),
        Pattern.compile("do\\s+not\\s+share", Pattern.CASE_INSENSITIVE),
        Pattern.compile("verification\\s+code", Pattern.CASE_INSENSITIVE),
        Pattern.compile("one[\\s-]?time\\s+(?:pin|password|code)", Pattern.CASE_INSENSITIVE),
        Pattern.compile("security\\s+code", Pattern.CASE_INSENSITIVE),
    };

    static boolean containsSensitiveSecret(String text) {
        if (text == null || text.isEmpty()) return false;
        for (Pattern p : SENSITIVE_PATTERNS) {
            if (p.matcher(text).find()) return true;
        }
        return false;
    }
}
