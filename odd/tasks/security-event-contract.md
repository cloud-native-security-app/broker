# Security event contract (broker repo)

Self-contained: any AI agent can start from this file with no prior context.

- Feature id: `security-event-contract` (task KAP-03 of the cross-repo master plan)
- Branch: `feature/security_event_contract` (cut from `origin/develop` 660f19f)
- Master plan: repo `infra`, path `odd/tasks/kinesis-admin-panel.md` (sections Decisions, Event contract, Constraints)
- Engram topic: `odd/security-event-contract/tasks` (project: broker). Mirror pending (written at close).
- Dependent tasks: KAP-06/07/08 (producers), KAP-09 (gateway consumer).

## Objective

Freeze the JSON Schema contract of the records carried by the Kinesis `security-events` stream, with a Rust
validator, tests, and one example per event type that producers reuse in their own tests.

## Scope

In: `contracts/security-event.schema.json`, `contracts/examples/security-event/*.json`, `validate_security_event`
in `src/contracts.rs`, `contracts/README.md`, decision record D-14, `docs/architecture.md` contract list.
Out: Kinesis publishing or consuming (KAP-06..09), Terraform stream (KAP-01), any shared crate.

## Constraints

- Events never carry credentials, tokens, raw scan output, or message bodies: `additionalProperties: false`
  everywhere, no free-text fields except short identifiers constrained by pattern.
- Partition key rule (documentation only, not enforced by the schema): `scan_id`, else `actor_sub`.
- No `schema_version`: `docs/architecture.md` forbids versioning the contract without discussing it first.
- Artifacts in English; `docs/decisions.md` entry in neutral Spanish. Conventional Commits, no attribution.

## Tooling

- TDD mode: on (source: user global config "Strict TDD Mode: enabled"). Runner: `make verify`.
- Route: one delegated writer (task KAP-03), no SDD.

## Assumptions

- A1: `scan_id` is a non-empty string (max 128), not a UUID, to match the existing contracts (`correlation_id`
  is a free string; the master plan said UUID).
- A2: `format` is only an annotation for the validator in draft 2020-12, so `event_id` and `occurred_at` also
  carry a `pattern`.
- A3: `confidence` enum is `confirmed|version_match` (nmap-service `domain.rs` `Confidence`); the broker
  `scan-outcome` schema does not define it yet.
- A4: `scan.cancelled` has an optional `target` (the cancellation message carries no IP); `scan.requested`
  requires it.
- A5: failure and dead-letter reasons are `reason_code` slugs (`^[a-z][a-z0-9_]{0,63}$`), never free text, so
  an SSH error string or message content cannot leak.

## Checklist

| ID | Task | Done |
|---|---|---|
| SEC-01 | Feature document | [x] 7538ae0 |
| SEC-02 | RED: failing tests for `validate_security_event` (valid per type, rejections) | [x] RED observed: `cargo test --lib` failed to compile (E0425, `validate_security_event` not found, 19 errors) |
| SEC-03 | GREEN: schema + validator + examples | [x] GREEN: `cargo test --lib` 33 passed; commit b7c4f9d |
| SEC-04 | Docs: `contracts/README.md`, D-14, architecture list | [x] docs commit (see Progress) |
| SEC-05 | `make verify` green | [x] exit 0 (fmt, clippy -D warnings, 33 unit tests, LocalStack integration, doc) |

## Progress

- 2026-11-09: SEC-01 feature doc (7538ae0). SEC-02/03 tests first (RED, compile error), then schema, validator and 11 examples (b7c4f9d, GREEN). SEC-04 docs commit. Commit review (gga) passed on each commit; its only note is a pre-existing `panic!` in `compile()`, out of scope.
- Review: native assessment declined per stored user policy (no permissions or secrets task).

## Next step

Push and PR of `feature/security_event_contract` are the user decision. Then KAP-06/07/08 can use `contracts/examples/security-event/`.
