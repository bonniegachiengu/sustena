package online.vyybandasky.sustena.smscapture;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

/**
 * The first gate on the handset: whose texts this app is allowed to read.
 *
 * <p>★★★ This is a privacy boundary before it is a feature. Everything that
 * fails this check is dropped inside {@code onReceive} and never reaches the
 * queue, the disk, or the network. A regression here does not corrupt a number
 * on a screen -- it reads someone's private messages. So the negative cases
 * below matter more than the positive ones, and there are deliberately more of
 * them.
 *
 * <p>★★ Sender only. Never the wording: several real KCB messages say "M-PESA"
 * in their own text, and a filter that read bodies would be deciding who to
 * listen to based on what was said.
 */
public class SmsSenderFilterTest {

    // ── the two senders this app exists to read ─────────────────────────────

    @Test
    public void mpesaIsRead() {
        assertTrue(SmsSenderFilter.isKnownFinancialSender("MPESA"));
    }

    @Test
    public void kcbIsRead() {
        assertTrue(SmsSenderFilter.isKnownFinancialSender("KCB"));
    }

    @Test
    public void caseDoesNotMatter() {
        // ★ Carriers are not consistent about case, and a filter that missed
        //   "MPesa" would silently stop reading real money messages -- a
        //   failure that looks like nothing happening.
        assertTrue(SmsSenderFilter.isKnownFinancialSender("MPesa"));
        assertTrue(SmsSenderFilter.isKnownFinancialSender("mpesa"));
        assertTrue(SmsSenderFilter.isKnownFinancialSender("kcb"));
    }

    @Test
    public void surroundingWhitespaceDoesNotMatter() {
        assertTrue(SmsSenderFilter.isKnownFinancialSender("  MPESA  "));
    }

    @Test
    public void aDecoratedSenderIdStillCounts() {
        // ★ Real sender ids are not always the bare word.
        assertTrue(SmsSenderFilter.isKnownFinancialSender("KCB-BANK"));
        assertTrue(SmsSenderFilter.isKnownFinancialSender("SAFARICOM-MPESA"));
    }

    // ── everyone else, which is the point ───────────────────────────────────

    @Test
    public void aPersonalNumberIsNeverRead() {
        // ★★★ The case this filter exists for. A friend's text is not the
        //     app's business and must not even be looked at.
        assertFalse(SmsSenderFilter.isKnownFinancialSender("+254712345678"));
    }

    @Test
    public void anotherBankIsNotRead() {
        // ★★ Not "all banks". Only the two this household actually banks with,
        //    because a wider net would be reading more than was asked for.
        assertFalse(SmsSenderFilter.isKnownFinancialSender("EQUITY"));
        assertFalse(SmsSenderFilter.isKnownFinancialSender("COOPBANK"));
        assertFalse(SmsSenderFilter.isKnownFinancialSender("ABSA"));
    }

    @Test
    public void marketingAndUtilitiesAreNotRead() {
        assertFalse(SmsSenderFilter.isKnownFinancialSender("SAFARICOM"));
        assertFalse(SmsSenderFilter.isKnownFinancialSender("KPLC"));
        assertFalse(SmsSenderFilter.isKnownFinancialSender("Google"));
    }

    @Test
    public void nullAndEmptyAreRefused() {
        // ★ A malformed broadcast must fail CLOSED. Defaulting to "read it"
        //   when the sender is unknown is exactly backwards for a privacy gate.
        assertFalse(SmsSenderFilter.isKnownFinancialSender(null));
        assertFalse(SmsSenderFilter.isKnownFinancialSender(""));
        assertFalse(SmsSenderFilter.isKnownFinancialSender("   "));
    }

    @Test
    public void aBodyMentioningMpesaDoesNotMakeTheSenderFinancial() {
        // ★★★ The wording-is-not-identity rule, asserted. If this ever passes
        //     a body-shaped string, someone has started deciding who to read
        //     by what was said rather than by who said it.
        assertFalse(SmsSenderFilter.isKnownFinancialSender("+254700000000"));
    }
}
