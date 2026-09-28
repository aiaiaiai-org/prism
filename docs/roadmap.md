# Roadmap

The roadmap is evidence-driven. Repository names describe ownership; implementation must prove the boundary.

## 0 — execution foundation (`complete`)

Prism has a working vertical slice:

- content variants and provider-neutral domain types;
- capability-driven preflight;
- explicit dispatch policies and idempotency derivation;
- provider registry and test provider;
- JSON/NDJSON execution protocol;
- schema and golden contract tests;
- Full CI and architecture checks.

## 1 — Threads provider proof (`current`)

Text-only Threads publishing is implemented behind injected binding and HTTP boundaries.

Next:

- run controlled live validation;
- add image, video, and carousel support;
- keep provider-specific behavior inside the adapter.

Instagram may later share private Meta transport/auth helpers while keeping a separate provider adapter. X and other providers remain independent.

## 2 — control plane (`next`)

`prism-hub` owns accounts, workspaces, OAuth lifecycle, scheduling, persistence, approvals, jobs, audit history, and the client-facing API. It invokes Prism through a versioned boundary and never owns provider HTTP behavior.

Implementation state for the hub is tracked in its own repository.

## 3 — explicit intelligence (`planned`)

`prism-ai` is an optional content-variant producer behind a hub-owned port. Generated output becomes explicit `ContentVariant` values with provenance before normal approval, preflight, and delivery.

## 4 — clients and deployment profiles (`planned`)

- `prism-bot` provides Telegram and future messaging clients against the hub API;
- `prism-panel` provides the planned web administration client;
- self-hosted, aiaiaiai-managed, and isolated deployments use the same versioned artifacts.

## 5 — reporting and peer integrations (`planned`)

Add optional archive, metrics, and reporting modules. Mail intelligence lives in `prism-mail`; HQBase is its current mail source behind focused source ports, and `prism-hub` consumes the resulting versioned artifacts. Client-domain concepts stay outside Prism core.

## Deferred decisions

- developer CLI, when a concrete workflow justifies it;
- canonical aiaiaiai infrastructure repository;
- exact `prism-hub` license;
- unattended HQBase service authorization;
- crates.io publication names and policy;
- daemon transport, only if measurements justify it.

Native panel applications, including `prism-panel-ios`, are outside this roadmap and require a separate product and architecture decision.

See [`status.md`](status.md) for Prism's exact implemented/planned boundary.

<!-- © 2026 aiaiaiai · aiaiaiai.org -->
