package online.vyybandasky.sustena;

/**
 * CRITICAL, security-relevant. The SECOND on-device detector, and the point is
 * that it fails differently from the first.
 *
 * <p><b>Three layers checking the same ten regexes are one layer written three
 * times.</b> SmsSecretFilter, the ingest engine and the transducer all consult
 * the same vocabulary, so when that vocabulary misses, all three miss together
 * for the same reason. Layering buys independence of FAILURE, not a count.
 *
 * <p><b>So this one reads no words at all.</b> It looks at the shape of the
 * message: is there a short bare code, is there a transaction reference, is
 * anything shouted, does any figure carry cents. A bank inventing new phrasing,
 * or a message in Swahili, defeats the vocabulary completely and leaves the
 * shape untouched -- which is the only sense in which a second layer is a second
 * layer.
 *
 * <p><b>Combined with the vocabulary by OR</b>, because the costs are not
 * symmetric: a false positive is one capture Bonnie re-enters by hand, and a
 * false negative puts a live credential on the wire. Either detector refusing is
 * a refusal.
 *
 * <p><b>Deliberately NOT a fourth copy of the vocabulary.</b> IMM-11's whole
 * complaint is that the phone, the engine and the transducer share one keyword
 * list; adding a fourth copy of that list would have deepened the coupling the
 * row exists to break. This is a different algorithm, so a change to one does
 * not silently need a change to the other.
 *
 * <p><b>Residual, stated honestly:</b> the shape rules are now themselves
 * hand-mirrored in two runtimes (this class and sustena-core's
 * {@code secret_shape.rs}), which is a smaller instance of the same problem.
 * It is smaller because it is five stable structural features rather than a
 * vocabulary that grows every time a bank writes a new sentence -- but it is
 * not zero, and the real fix is one shared check the phone can call.
 *
 * <p>Calibrated against the real M-Pesa and KCB corpus. Note the real OTP
 * sample NAMES A USD AMOUNT, so the obvious rule "a credential message does not
 * mention money" would have missed the one message it was written for; it is
 * caught on noCents instead, because money is quoted to two places and 113.8 is
 * not.
 */
final class SmsShapeFilter {
    private SmsShapeFilter() {}

    /** Words above which a message stops being brief. Set from the corpus. */
    private static final int BRIEF_WORDS = 25;

    /**
     * Corroborating features required alongside a bare code.
     *
     * <p>Three of four. At two, a real M-Pesa paybill confirmation -- which
     * genuinely contains a bare eight-digit account number -- starts to fire,
     * and a detector that refuses real payments is one somebody turns off.
     */
    private static final int REQUIRED_CORROBORATION = 3;

    /**
     * Does this message have the shape of one delivering a credential?
     *
     * <p>A bare code is NECESSARY and not sufficient.
     */
    static boolean looksLikeASecret(String text) {
        if (text == null || text.isEmpty()) return false;
        String[] raw = text.trim().split("\\s+");
        if (!hasBareCode(raw)) return false;

        int corroboration = 0;
        if (noTransactionReference(raw)) corroboration++;
        if (raw.length <= BRIEF_WORDS) corroboration++;
        if (hasShoutedRun(raw)) corroboration++;
        if (!hasCents(text)) corroboration++;
        return corroboration >= REQUIRED_CORROBORATION;
    }

    /**
     * A standalone 4-8 character token of digits that is not a date, a time or
     * an amount.
     */
    private static boolean hasBareCode(String[] raw) {
        for (String r : raw) {
            if (isDateOrTime(r)) continue;
            // Trim the sentence's own punctuation FIRST. A full stop at the end
            // of "000000." is a sentence ending, not a decimal point -- reading
            // it as one made an earlier version of this rule miss a real TAN
            // code, which is how this line came to be written.
            String t = trimPunctuation(r);
            if (hasInnerSeparator(t)) continue;   // an amount, not a code
            if (t.length() >= 4 && t.length() <= 8 && isAllDigits(t)) return true;
        }
        return false;
    }

    /** Real confirmations carry a reference; a credential rarely does. */
    private static boolean noTransactionReference(String[] raw) {
        for (String r : raw) {
            String t = trimPunctuation(r);
            if (t.length() >= 8 && isAlnum(t) && hasDigit(t) && hasLetter(t)) return false;
        }
        return true;
    }

    /** Three consecutive shouted words. Two is a name. */
    private static boolean hasShoutedRun(String[] raw) {
        int run = 0;
        for (String r : raw) {
            String t = trimPunctuation(r);
            if (t.length() > 1 && isAllUpper(t)) {
                run++;
                if (run >= 3) return true;
            } else {
                run = 0;
            }
        }
        return false;
    }

    /** Money is quoted to two places; a code is not a quantity. */
    private static boolean hasCents(String text) {
        char[] b = text.toCharArray();
        for (int i = 0; i < b.length; i++) {
            if (b[i] != '.') continue;
            boolean before = i > 0 && Character.isDigit(b[i - 1]);
            boolean two = i + 2 < b.length
                    && Character.isDigit(b[i + 1]) && Character.isDigit(b[i + 2]);
            boolean noThird = i + 3 >= b.length || !Character.isDigit(b[i + 3]);
            if (before && two && noThird) return true;
        }
        return false;
    }

    /**
     * Checked on the RAW token, because the separators are the whole signal --
     * "20/7/26" and "2:15" are only distinguishable from a code by them.
     */
    private static boolean isDateOrTime(String raw) {
        boolean sawSep = false;
        for (char c : raw.toCharArray()) {
            if (c == '/' || c == ':' || c == '-') {
                sawSep = true;
            } else if (!Character.isDigit(c)) {
                return false;
            }
        }
        return sawSep;
    }

    /** A separator between two digits -- the mark of an amount, not a code. */
    private static boolean hasInnerSeparator(String t) {
        for (int i = 1; i + 1 < t.length(); i++) {
            char c = t.charAt(i);
            if ((c == '.' || c == ',')
                    && Character.isDigit(t.charAt(i - 1))
                    && Character.isDigit(t.charAt(i + 1))) {
                return true;
            }
        }
        return false;
    }

    private static String trimPunctuation(String s) {
        int a = 0, b = s.length();
        while (a < b && !Character.isLetterOrDigit(s.charAt(a))) a++;
        while (b > a && !Character.isLetterOrDigit(s.charAt(b - 1))) b--;
        return s.substring(a, b);
    }

    private static boolean isAllDigits(String s) {
        if (s.isEmpty()) return false;
        for (char c : s.toCharArray()) if (!Character.isDigit(c)) return false;
        return true;
    }

    private static boolean isAlnum(String s) {
        for (char c : s.toCharArray()) if (!Character.isLetterOrDigit(c)) return false;
        return !s.isEmpty();
    }

    private static boolean hasDigit(String s) {
        for (char c : s.toCharArray()) if (Character.isDigit(c)) return true;
        return false;
    }

    private static boolean hasLetter(String s) {
        for (char c : s.toCharArray()) if (Character.isLetter(c)) return true;
        return false;
    }

    private static boolean isAllUpper(String s) {
        for (char c : s.toCharArray()) {
            if (!Character.isLetter(c) || !Character.isUpperCase(c)) return false;
        }
        return !s.isEmpty();
    }

    /**
     * The combined on-device gate: either detector refusing is a refusal.
     *
     * <p>OR, not AND. An AND would let each layer veto the other's catch, which
     * is the opposite of layering.
     */
    static boolean mustNotLeaveTheDevice(String text) {
        return SmsSecretFilter.containsSensitiveSecret(text) || looksLikeASecret(text);
    }
}
