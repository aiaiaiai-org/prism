# Implementation status

This document separates behavior implemented in Prism from behavior that is planned or owned by other products.

## Implemented now

| Surface | Current behavior |
| --- | --- |
| `prism-core` | Content variants, capabilities, requests, validation, outcomes, receipts, events, and portable post/message surfaces |
| `prism-provider` | Async provider adapter contract and deterministic registry |
| `prism-provider-threads` | Text-only official Threads adapter with injected binding resolution and HTTPS transport |
| `prism-provider-meta` | Instagram single-image feed, Facebook Page text, and WhatsApp individual text-message adapters with injected bindings/transports |
| `prism-protocol` | `prism-execution.v1` request/response envelopes and generated JSON Schema |
| `prism-runtime` | Stateless `capabilities`, `validate`, and `publish` operations over JSON/NDJSON |
| `prism-testkit` | Test provider, safe call recording, and adapter conformance helpers |
| `xtask` | Contract generation, schema drift checks, and repository policy checks |

The live adapters can call official production APIs only when an application supplies the required bindings and transport/media resolution. Required CI supplies no live credentials or targets.

## Meta provider family

| Provider | Implemented v1 capability |
| --- | --- |
| Threads | text feed post |
| Instagram | exactly one image feed post with optional caption |
| Facebook | Page text feed post |
| WhatsApp | individual Business Platform text message |

WhatsApp uses `PublicationFormat::Message`, not `Post`: it is recipient-addressed messaging rather than a feed publication. The configured channel binding owns the recipient relationship.

## Execution

For `validate` and `publish`, Prism:

1. validates request structure;
2. resolves one explicit eligible content variant per target;
3. resolves the provider adapter and capability snapshot;
4. validates provider-neutral limits and namespaced options;
5. runs provider preflight without publishing;
6. finishes preflight for every target;
7. applies the dispatch policy;
8. publishes eligible targets and returns ordered outcomes and events.

`validate` stops after preflight. `capabilities` returns one provider capability snapshot.

### Dispatch policies

| Policy | Guarantee |
| --- | --- |
| `require_all_valid` | If any target fails preflight, Prism performs no external action. Valid targets become `skipped`. |
| `independent` | Every valid target is attempted. Invalid and provider-failed targets remain isolated. |

Prism does not promise cross-provider atomicity.

## Determinism and idempotency

- target, outcome, and event order is deterministic;
- variant selection is explicit;
- target idempotency material uses versioned `prism-idempotency.v1` scope;
- adapters may map or hash that material to provider limits;
- Prism has no durable deduplication store or retry scheduler.

The implemented Meta publish paths do not claim native idempotency. If a final external action returns an ambiguous outcome, Prism returns `outcome_unknown`; callers must reconcile provider state before retrying. Instagram container creation is distinct because it does not make content public and may be retried safely under the adapter contract.

## Security

- protocol and domain values carry opaque credential references, never raw provider tokens;
- provider options must use the selected provider namespace;
- secret-bearing extension keys are rejected;
- stdout is reserved for protocol envelopes;
- diagnostics use stderr without request payloads;
- provider adapters own secret resolution and upstream redaction;
- Meta token wrappers are non-serializable and redacted in debug output;
- signed Instagram media URLs are redacted in debug output;
- provider requests require HTTPS bearer authorization;
- WhatsApp recipients are resolved through channel bindings, not arbitrary request options.

## Verification

Full CI runs formatting, Clippy with warnings denied, workspace tests, generated-contract drift checks, copyright/license checks, rustdoc with warnings denied, and a Rust 1.85 minimum-supported-version check. Dependencies are locked. Golden fixtures protect the wire contract. Required CI never performs a live publish.

## Not implemented

Prism does not currently provide:

- Threads image, video, carousel, poll, reply, or provider-specific option publishing;
- Instagram video, carousel, story, Reel, or alt-text preservation;
- Facebook Page image/video/Reels publishing or personal-profile posting;
- WhatsApp templates, media, groups, broadcasts, Status publishing, or general conversation orchestration;
- X, Telegram, Mastodon, or other non-Meta live provider adapters;
- OAuth flows, token storage/refresh, account discovery, or production credential persistence;
- general media fetching, transformation, upload, or durable storage;
- retries, backoff, scheduling, queues, or durable idempotency records;
- accounts, workspaces, approvals, audit history, or reporting;
- a remote HTTP service or daemon lifecycle;
- deployment artifacts or hosted infrastructure.

The control plane, ai generation, OAuth/account lifecycle and clients belong to separate Prism repositories. Their implementation state is tracked there, not here.

## Next Prism increments

1. Run controlled live validation for each configured Meta provider and record evidence.
2. Extend Threads and Instagram media capability only when media-resolution and recovery semantics stay explicit.
3. Add Facebook media surfaces and WhatsApp templates/media as independent capability increments.
4. Add a non-Meta provider to prove the provider-neutral boundary beyond one vendor family.

## Deferred decisions

- first non-Meta provider;
- public crate names and crates.io publication policy;
- daemon, Unix socket, or HTTP runtime transport;
- delivery concurrency and retry semantics;
- optional developer CLI.

See [`architecture.md`](architecture.md), [`content-variants.md`](content-variants.md), [`engineering-principles.md`](engineering-principles.md), [`protocol.md`](protocol.md), [`providers/meta.md`](providers/meta.md), and [`roadmap.md`](roadmap.md).

<!-- © 2026 aiaiaiai · aiaiaiai.org -->
