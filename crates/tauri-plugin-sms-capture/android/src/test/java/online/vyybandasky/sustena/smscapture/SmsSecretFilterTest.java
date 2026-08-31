package online.vyybandasky.sustena.smscapture;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

/**
 * The second gate, and the one with the sharpest consequence.
 *
 * <p>★★★ An OTP arrives from the SAME sender id as a real payment
 * confirmation, so the sender filter cannot catch it -- this has to read the
 * body. A message that trips this is dropped inside {@code onReceive}: never
 * queued, never written to disk, never sent anywhere, not even hashed. There is
 * no code path that stores it and then decides.
 *
 * <p>★★ Two failure directions, and they are not symmetric. Missing a secret
 * leaks a live credential off the handset. Over-matching silently swallows a
 * real transaction, and the household never learns the money moved. So this
 * file tests both, and the false-positive half is written against real
 * transaction wording rather than invented text.
 */
public class SmsSecretFilterTest {

    // ── secrets, which must never leave the phone ───────────────────────────

    @Test
    public void aPlainOtpIsDropped() {
        assertTrue(SmsSecretFilter.containsSensitiveSecret(
                "Your OTP is 123456. Do not share it with anyone."));
    }

    @Test
    public void theDoNotShareWarningAloneIsEnough() {
        // ★ The phrase a bank puts on anything sensitive. Enough on its own,
        //   because the cost of honouring it wrongly is one manual paste.
        assertTrue(SmsSecretFilter.containsSensitiveSecret(
                "Use 4417 to continue. Do not share this with anyone."));
    }

    @Test
    public void verificationAndOneTimeWordingIsDropped() {
        assertTrue(SmsSecretFilter.containsSensitiveSecret("Your verification code is 8842"));
        assertTrue(SmsSecretFilter.containsSensitiveSecret("Your one-time password is 1122"));
        assertTrue(SmsSecretFilter.containsSensitiveSecret("Your one time pin is 9080"));
    }

    @Test
    public void bankSpecificSecretWordingIsDropped() {
        // ★★ These came from real KCB messages. The earlier speculative list
        //    matched none of them, which is why the wording here is taken from
        //    what actually arrived rather than from what seemed likely.
        assertTrue(SmsSecretFilter.containsSensitiveSecret("Your TAN code is 5567"));
        assertTrue(SmsSecretFilter.containsSensitiveSecret("Activation code: 7781, valid for 90s"));
        assertTrue(SmsSecretFilter.containsSensitiveSecret("Your card secret PIN is 4410"));
        assertTrue(SmsSecretFilter.containsSensitiveSecret("Your security code is 3321"));
        assertTrue(SmsSecretFilter.containsSensitiveSecret("Your PIN is 0091"));
        assertTrue(SmsSecretFilter.containsSensitiveSecret("This code is valid for 5 minutes"));
    }

    @Test
    public void caseDoesNotLetASecretThrough() {
        assertTrue(SmsSecretFilter.containsSensitiveSecret("your otp is 4432"));
        assertTrue(SmsSecretFilter.containsSensitiveSecret("DO NOT SHARE THIS CODE"));
    }

    // ── real money messages, which must NOT be swallowed ────────────────────

    @Test
    public void arealMpesaReceiptIsKept() {
        // ★★★ The other failure direction. If this ever starts returning true,
        //     the household stops being told its money moved -- and it fails
        //     silently, which is the worst way for a money app to fail.
        assertFalse(SmsSecretFilter.containsSensitiveSecret(
                "TFF9J0ABCD Confirmed. Ksh1,500.00 sent to NAIVAS SUPERMARKET "
                        + "on 1/8/26 at 11:15 AM. New M-PESA balance is Ksh12,154.47. "
                        + "Transaction cost, Ksh0.00."));
    }

    @Test
    public void aRealKcbCardTransactionIsKept() {
        assertFalse(SmsSecretFilter.containsSensitiveSecret(
                "KES 429.00 spent on your KCB card at GOOGLE Spotify Music. "
                        + "Avail balance KES 59,055.00"));
    }

    @Test
    public void aRealPaybillDepositIsKept() {
        assertFalse(SmsSecretFilter.containsSensitiveSecret(
                "Ksh 40000.00 sent to KCB Pay Bill 522522 for account 135***5140 "
                        + "has been received on 01/08/2026 at 12:21 PM. M-PESA ref UH1B91GYNW"));
    }

    @Test
    public void aBalanceEnquiryIsKept() {
        // ★ Contains no secret, though it is about an account. "PIN" appearing
        //   as a bare noun elsewhere must not be enough -- the pattern is
        //   "pin is", not "pin".
        assertFalse(SmsSecretFilter.containsSensitiveSecret(
                "Your KCB account balance is KES 59,055.00 as at 01/08/2026"));
    }

    @Test
    public void nullAndEmptyAreNotSecrets() {
        // ★ Fails OPEN here, unlike the sender filter, and deliberately: an
        //   empty body carries nothing to leak, and treating it as a secret
        //   would drop malformed-but-real messages instead of surfacing them.
        assertFalse(SmsSecretFilter.containsSensitiveSecret(null));
        assertFalse(SmsSecretFilter.containsSensitiveSecret(""));
    }
}
