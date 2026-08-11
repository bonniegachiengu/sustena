# Security Policy

## Reporting a vulnerability

**Please do not open a public issue for a security problem.**

Report it privately to **bonniegachiengu@gmail.com** with:

- what the issue is,
- how to reproduce it,
- what an attacker could do with it.

You will get an acknowledgement within **72 hours**. Sustena is maintained by one person, so please allow reasonable time for a fix before any public disclosure. You will be credited unless you would rather not be.

---

## Supported versions

Sustena is **pre-1.0** and under active architectural change. Only the current `main` branch receives security fixes. There are no maintained release branches yet.

---

## Scope

Sustena handles personal financial records and reads financial SMS on Android, so the following are treated as high severity:

- authentication or session handling (token forgery, missing revocation, privilege escalation),
- one account reaching another account's data,
- anything that writes state while bypassing the enforcement gate,
- anything that causes an outbound effect without an explicit human confirmation,
- leakage of captured message content, particularly one-time passcodes.

**Out of scope:** issues that require an attacker to already have physical access to an unlocked device, and findings against a deployment's own misconfiguration rather than this code.

---

## Design notes relevant to security

Two properties are enforced structurally rather than by convention, and both are deliberate:

- **One write path.** Every state change passes the same enforcement check before it persists. A refusal commits nothing at all.
- **Human-gated egress.** Nothing leaves the system without an explicit confirmation step. There is no code path that moves money.

On Android, captured messages are filtered on-device before anything is transmitted: only known financial senders are considered, and messages matching one-time-passcode and PIN patterns are discarded before they leave the phone. If you find a message shape that defeats that filter, treat it as a high-severity report — it is exactly the case the filter exists to catch.

---

## Configuration warning for operators

If you deploy Sustena yourself, note that the application's configuration defaults are **development defaults**, and they are visible in this public repository. A real deployment must set its own `SECRET_KEY` and `ADMIN_TOKEN` from the environment and must set `ENVIRONMENT=production`. Do not rely on the built-in defaults for anything reachable from a network.
