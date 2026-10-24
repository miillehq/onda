# Official OpenAI references

Retrieved and inspected on **2026-10-03**, including each page's Markdown form. Implementation authority is the current official documentation; this record makes future protocol changes reviewable.

| Source | Used for |
| --- | --- |
| [Open-source plan usage overview](https://developers.openai.com/siwc/token-sharing-open-source) | Public client registration and stable host identity |
| [Registration and sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in) | Dynamic registration, loopback URI, PKCE, granted scopes, issued client IDs |
| [Accounts and sessions](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions) | Separate registrations, rotating refresh serialization, revocation, credential handling |
| [Models and inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference) | Account-specific catalog shape and successful streaming completion |
| [Preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations) | Required request fields and unsupported inputs, parameters, tools |
| [Token reference](https://developers.openai.com/siwc/token-sharing-open-source/token-reference) | Token fields, expiry, replacement refresh tokens, opaque metadata |
| [Errors and recovery](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery) | Plan permission, remote limits, transient failures, terminal refresh errors |
| [Website OIDC verification](https://developers.openai.com/siwc/website) | JWKS signature verification, issuer/audience/time/nonce checks |
| [Production OIDC discovery](https://auth.openai.com/.well-known/openid-configuration) | Exact issuer, authorization, token, JWKS, and revocation endpoints; RS256 |

The website identity-only commercial flow has different client provisioning. Eagle uses its JWT-validation guidance without importing that flow's confidential-client assumptions. No upstream application token format, private ChatGPT backend endpoint, or embedded partner credential is used.

For future maintenance, reread the relevant pages before changing OAuth scope, endpoint, revocation, or inference behavior. If an official endpoint changes, update the explicit allowlist and mocked contract together. Never make production endpoint overrides user-configurable to accommodate undocumented protocols.
