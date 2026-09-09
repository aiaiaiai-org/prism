# Prism

Prism is a provider-agnostic publishing engine for deterministic multi-channel delivery.

One publication can have several explicit variants for different languages, voices, audiences, providers, and formats. Prism validates each target before it publishes anything.

**one intent → explicit variants → deterministic delivery**

## What Prism owns

- explicit `ContentVariant` values;
- provider capabilities and preflight validation;
- deterministic target selection and dispatch;
- typed delivery outcomes and retry safety;
- provider adapters behind one stable contract;
- the versioned `prism-execution.v1` JSON/NDJSON protocol.

Prism does not own accounts, OAuth lifecycle, scheduling, persistence, client UI, infrastructure, or ai generation.

## Workspace

| Package | Owns |
| --- | --- |
| `prism-core` | Content variants, capabilities, requests, outcomes, and errors |
| `prism-provider` | Provider adapter contract and registry |
| `prism-provider-threads` | Official Threads text adapter |
| `prism-provider-meta` | Instagram single-image feed, Facebook Page text, WhatsApp direct-message, and WhatsApp Channel text-post boundaries |
| `prism-protocol` | `prism-execution.v1` envelopes and JSON Schema |
| `prism-runtime` | Stateless preflight and dispatch over JSON/NDJSON |
| `prism-testkit` | Test provider and conformance helpers |
| `xtask` | Contract generation and repository checks |

## Try it

The opt-in test provider performs no network action:

```bash
cargo run -p prism-runtime --features test-provider -- --json \
  < contracts/examples/publish.request.json
```

Without `--json`, the runtime reads and writes one NDJSON envelope per line. stdout is protocol-only; diagnostics use stderr.

## Documentation

| Document | Purpose |
| --- | --- |
| [`content variants`](docs/content-variants.md) | Locale, voice profile, audience, provider, format, and provenance |
| [`architecture`](docs/architecture.md) | Core ownership and execution boundaries |
| [`ecosystem`](docs/ecosystem.md) | Boundaries between Prism and peer products |
| [`engineering principles`](docs/engineering-principles.md) | SOLID, object boundaries, and dependency rules |
| [`protocol`](docs/protocol.md) | `prism-execution.v1` transport and compatibility |
| [`Meta providers`](docs/providers/meta.md) | Threads, Instagram, Facebook and WhatsApp adapter semantics |
| [`Threads provider`](docs/providers/threads.md) | Threads adapter capabilities and retry safety |
| [`implementation status`](docs/status.md) | What exists now and what does not |
| [`roadmap`](docs/roadmap.md) | Evidence-driven implementation order |
| [`copyright and licensing`](docs/licensing.md) | Apache-2.0 and aiaiaiai copyright policy |

## Ecosystem

`prism-hub` is the control plane. Clients such as `prism-bot` and the planned `prism-panel` talk to the hub, not to Prism or provider APIs. Optional generation belongs to `prism-ai` and produces explicit content variants before delivery.

See [`docs/ecosystem.md`](docs/ecosystem.md) for the complete dependency direction.

## Status

Prism is pre-1.0. The deterministic execution foundation and four Meta product boundaries are implemented: Threads text posts, Instagram single-image feed posts, Facebook Page text posts, and WhatsApp with destination-specific direct-message or Channel-post semantics. Direct messages use the official WhatsApp Cloud API transport. Channel posts use an explicit injected publisher boundary until Meta exposes a documented Channels publishing API. Live use still requires application-supplied bindings, credentials, media resolution where applicable, and provider-side account/app approval.

Wire and crate APIs follow semantic versioning. Protocol versions are independent from binary versions.

Licensed under Apache-2.0.

<!-- © 2026 aiaiaiai · aiaiaiai.org -->
