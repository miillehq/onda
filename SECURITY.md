# Security policy

Onda hears what you say, sees the text you select and can hold AI provider keys and ChatGPT
sign-in tokens, so security reports matter here. Thank you for making one.

## Report privately

Use GitHub's private vulnerability reporting: open the
[Security tab](https://github.com/miillehq/onda/security) and choose **Report a vulnerability**
([direct link](https://github.com/miillehq/onda/security/advisories/new)). Only the maintainers
see the report, and the fix is discussed in the same private advisory.

Please don't put security problems in public issues, pull requests or discussions. If the
reporting button is missing, open an issue that asks for a private contact, without any details.

Include what you can:

- what an attacker gains, and from where (another app, another user, a web page, the network);
- the steps or a proof of concept;
- the Onda version (Settings › Updates and help) and your operating system.

Never send real dictated text, recordings, API keys or tokens, yours or anyone's. Test data shows
the problem just as well.

## What is in scope

- dictated text, recordings, selected text or History leaving where
  [docs/security.md](docs/security.md) says they stay, including through logs or diagnostics;
- reading or stealing API keys, ChatGPT tokens or the History key;
- typing into the wrong window or into a password field;
- the interface reaching more of the system than its typed commands allow;
- the ChatGPT sign-in flow, AI provider requests and model downloads (integrity and redirects);
- the installers and the updater.

## What is not

The limits listed in [docs/security.md](docs/security.md), for example:

- malware already running as the same user;
- elevated windows that Onda refuses to type into;
- clipboard managers that ignore the privacy markers.

Release builds are currently unsigned on every platform (on macOS by decision, see
[PLAN.md](PLAN.md); on Windows until a certificate exists), so a report that they are unsigned is
already known.

## Supported versions

Onda is before 1.0. Fixes land on `master` and in the next release; older builds are not patched.
