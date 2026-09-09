# Meta provider family

Prism treats Meta products as separate provider semantics behind one provider-neutral execution contract. Shared HTTP/error helpers are private implementation detail; provider IDs, capabilities, bindings, validation and external actions remain independent.

| Provider | ID | Prism format | Implemented v1 action |
| --- | --- | --- | --- |
| Threads | `meta.threads` | `post` | text post |
| Instagram | `meta.instagram` | `post` | one-image Professional account feed post with optional caption |
| Facebook | `meta.facebook` | `post` | Page text feed post |
| WhatsApp | `meta.whatsapp` | `message` | individual Business Platform text message |

## Boundaries

Prism never stores OAuth state or raw credentials. A provider adapter receives opaque `ChannelRef` and `CredentialRef` values and resolves them through an injected binding resolver. The resolver must preserve the configured account/destination relationship for the complete execution.

Instagram additionally resolves an opaque `MediaRef` to a provider-fetchable public HTTPS URL inside the adapter boundary. Signed URL material is redacted from debug formatting.

WhatsApp is deliberately not modelled as a feed post. The configured channel binding resolves the sending phone-number ID and one individual recipient together with the credential. Recipient routing is not accepted through free-form provider options.

## Failure semantics

Provider error payloads are classified without exposing raw upstream messages. Authentication and rate-limit errors remain typed. Deterministic provider rejection is terminal for that request.

When the final public action may have reached Meta but the response is transient or unreadable, Prism returns `outcome_unknown`. Callers must reconcile provider state before retrying. Instagram container creation is retryable because the container is not yet public; ambiguity during `media_publish` is not.

## Current limitations

- Threads remains text-only in its existing adapter.
- Instagram v1 requires exactly one image and does not preserve alt text yet.
- Facebook v1 targets Pages only and is text-only.
- WhatsApp v1 targets individual recipients only and sends free-form text. Template eligibility, conversation-window rules, recipient consent and other Business Platform policy remain provider/account preconditions.
- OAuth UI, token refresh/storage, account discovery, app review, media storage and deployment belong outside Prism.
- Required CI uses injected fake boundaries and performs no live provider calls.

The Graph transports for Instagram, Facebook and WhatsApp default to API version `v26.0`; the base URL remains injectable for tests and controlled compatibility work.

<!-- © 2026 aiaiaiai · aiaiaiai.org -->
