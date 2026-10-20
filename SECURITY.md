# Security

Waldo hands an AI model the controls of a Mac. Its gates are what make that safe to try. A way past them is a security bug.

## Report it privately

Use GitHub's private reporting: open the Security tab of this repository, then choose "Report a vulnerability". Or write to info@justusewaldo.com.

Do not open a public issue, pull request, or discussion for it.

Say what you ran, on which Mac and which macOS, and what happened. A short script or a screen recording helps.

## What counts

These are security bugs:

- **A gate bypass.** A gated act happens without your yes: a payment, a message sent as you, a password typed or read, or a destructive change.
- **An approval without a real click.** The gate panel accepts a key press, a click posted by software, a press through the accessibility API, or anything else that is not your own mouse or trackpad.
- **A stop-switch bypass.** A tool acts while the stop switch is on. Or the corner gesture, `waldo stop`, or Stop everything fails to stop every session.
- **An audit gap.** A tool call or a gate answer leaves no line in `audit.jsonl`. Or the log keeps what it must not, such as the text of a password.

These are not, because the README's [Limits](README.md#limits) already say so:

- A session that uses its own shell to post events or to edit `policy.json`. The gates are guard rails, not a jail.
- An app with no accessibility tree, where only the place rules and the model's own `confirm` apply.

## What happens next

We confirm the bug, fix it on a private branch, and ship the fix in a release. Then we publish an advisory. It credits you, unless you ask us not to.

## Supported versions

Only the latest release gets security fixes.
