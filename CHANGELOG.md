# Changelog

All notable Prism changes are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses semantic versioning.

## [Unreleased]

### Added

- Deterministic content-variant, capability, and delivery domain model.
- Two-phase preflight and dispatch with explicit `require_all_valid` and `independent` policies.
- Object-safe asynchronous provider adapter contract and scriptable test provider.
- Versioned `prism-execution.v1` JSON/NDJSON process protocol.
- Generated JSON Schemas, golden fixtures, and contract drift checks.
- Canonical aiaiaiai copyright notices and automated policy validation.
- Architecture, ecosystem, status, licensing, security, contribution, and content-variant documentation.
- Text-only official Threads adapter with injected channel and credential resolution, redacted HTTP transport, capability snapshot, and fixture-based tests.
- `prism-provider-meta` with separate Instagram single-image feed, Facebook Page text, and WhatsApp Business individual text-message adapters.
- Portable `message` publication format for recipient-addressed delivery such as WhatsApp.
- Redacted Meta access tokens, signed-media URLs, Graph API error mapping, and provider-specific binding/transport boundaries.
- `outcome_unknown` for external actions that may have succeeded and must be reconciled before retry.
- Locked dependencies and a Rust 1.85 minimum-supported-version gate.

### Changed

- Target idempotency material is collision-free and versioned as `prism-idempotency.v1`.
- Delivery errors may include namespaced safe recovery details, such as a Threads or Instagram container ID after an ambiguous publish response.
- Canonical repository metadata uses `aiaiaiai-org/prism`.
- Human-facing documentation uses the en_SV writing style and canonical Prism terminology.

### Migration

- Consumers that exhaustively match `DeliveryErrorClass` must handle `outcome_unknown` and block automatic retry until provider state is reconciled.
- Consumers that exhaustively match `PublicationFormat` must handle the new `message` variant. The `prism-execution.v1` protocol remains versioned independently from crate releases.
- Consumers that persisted the previous `root:target` idempotency string must migrate to `prism-idempotency.v1`. Protocol versions remain independent from crate versions.

<!-- © 2026 aiaiaiai · aiaiaiai.org -->
