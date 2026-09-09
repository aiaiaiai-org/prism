# Meta provider family

Prism treats Meta products as separate provider semantics behind one provider-neutral execution contract. Shared HTTP/error helpers are private implementation detail; provider IDs, capabilities, bindings, validation and external actions remain independent.

| Provider | ID | Prism format | Implemented v1 action |
| --- | --- | --- | --- |
| Threads | `meta.threads` | `post` | text post |
| Instagram | `meta.instagram` | `post` | one-image Professional account feed post with optional caption |
| Facebook | `meta.facebook` | `post` | Page text feed post |
| WhatsApp | `meta.whatsapp` | `message` or `post` | individual Business Platform text message or text Channel update |

## Boundaries

Prism never stores OAuth state or raw credentials. A provider adapter receives opaque `ChannelRef` and `CredentialRef` values and resolves them through injected provider boundaries. The resolver must preserve the configured account/destination relationship for the complete execution.

Instagram additionally resolves an opaque `MediaRef` to a provider-fetchable public HTTPS URL inside the adapter boundary. Signed URL material is redacted from debug formatting.

WhatsApp has two distinct destination semantics under one provider ID:

- an individual target advertises only `PublicationFormat::Message` and resolves the sending phone-number ID, one recipient and the Meta credential atomically;
- a Channel target advertises only `PublicationFormat::Post` and delegates the public update to an injected `WhatsAppChannelPublisher`.

The target kind comes from the configured `ChannelRef` through `WhatsAppDestinationResolver`; Prism does not infer it from content or accept recipient/channel routing through free-form provider options.

Direct messages continue to use the official WhatsApp Cloud API messages endpoint. As of September 2026, Meta documents WhatsApp Channels in the WhatsApp/WhatsApp Business apps but does not document a WhatsApp Cloud API endpoint for programmatic Channel publishing. Prism therefore does not pretend that Channel updates use an official Meta Graph transport. The injected Channel publisher is an explicit integration boundary that can be backed by a controlled bridge today and replaced by an official Meta transport later without changing the Prism `post` contract.

## Failure semantics

Provider error payloads are classified without exposing raw upstream messages. Authentication and rate-limit errors remain typed. Deterministic provider rejection is terminal for that request.

When the final public action may have reached Meta but the response is transient or unreadable, Prism returns `outcome_unknown`. Callers must reconcile provider state before retrying. Instagram container creation is retryable because the container is not yet public; ambiguity during `media_publish` is not. A `WhatsAppChannelPublisher` implementation has the same obligation for ambiguous final Channel updates.

## Current limitations

- Threads remains text-only in its existing adapter.
- Instagram v1 requires exactly one image and does not preserve alt text yet.
- Facebook v1 targets Pages only and is text-only.
- WhatsApp direct messaging remains free-form text only. Template eligibility, conversation-window rules, recipient consent and other Business Platform policy remain provider/account preconditions.
- WhatsApp Channel posting is text-only in Prism v1 and requires an application-supplied Channel publisher because no official Cloud API Channels publishing endpoint is currently documented.
- WhatsApp Channel media, polls, edit/delete and metrics are not implemented yet.
- OAuth UI, token refresh/storage, account discovery, app review, media storage and deployment belong outside Prism.
- Required CI uses injected fake boundaries and performs no live provider calls.

The Graph transports for Instagram, Facebook and WhatsApp direct messaging default to API version `v26.0`; the base URL remains injectable for tests and controlled compatibility work.

<!-- © 2026 aiaiaiai · aiaiaiai.org -->
