# Finding port (broker repo)

Self-contained: any AI agent can start from this file with no prior context.

- Feature id: `finding-port` (task KAP-07b of the cross-repo master plan)
- Branch: `feature/finding_port` (cut from `origin/develop` 747454c)
- Sibling repos, same feature id: `nmap-service`, `gateway`, `user-service` (each has its own `odd/tasks/finding-port.md`).
- Dependent task: KAP-07 follow-up (`finding.critical` carries the real port).

## Objective

`VulnFinding` has no port, so the `finding.critical` event always carries `port: 0, protocol: "tcp"` and reports
cannot say where a vulnerability is. Add an optional `port`/`protocol` to `vulnFinding` and make both nullable in
`finding.critical`. Also reconcile `vulnFinding` with the fields `nmap-service` really emits.

## Scope

In: `contracts/scan-outcome.schema.json` (`vulnFinding`), `contracts/security-event.schema.json`
(`findingCriticalPayload`), the `finding.critical` example, tests in `src/contracts.rs`, `contracts/README.md`,
decision record D-15.
Out: producers and consumers (each sibling repo), Terraform, any shared crate.

## Constraints

- Backward compatible: any merge order must keep every repo green. All new fields are optional.
- Absent and `null` both mean "host-level finding / unknown port" (e.g. the authenticated SSH patch check).
- `additionalProperties: false` stays; no `schema_version` (forbidden by `docs/architecture.md`).
- Artifacts in English; `docs/decisions.md` entry in neutral Spanish. Conventional Commits, no attribution.

## Tooling

- TDD mode: on (source: user global config "Strict TDD Mode: enabled"). Runner: `make verify` (`cargo test` for the loop).
- Route: one delegated writer (task KAP-07b), no SDD.

## Reconciled `vulnFinding` fields

Source of truth: `nmap-service/src/domain.rs` `VulnFinding` serde attributes.

| Field | Required | Notes |
|---|---|---|
| `id`, `severity`, `description`, `nse_script`, `source`, `references` | yes | unchanged |
| `confidence` | no | `confirmed` / `version_match`; omitted by nmap-service when unknown (`skip_serializing_if`) (was missing) |
| `patch_state` | no | `unpatched` / `fixed`; omitted when not verified (was missing) |
| `port` | no | integer 1..65535, new; absent or `null` = host-level |
| `protocol` | no | `tcp` / `udp`, new; absent or `null` = host-level |

## Assumptions

- A1: `finding.critical` keeps `minimum: 0` for `port` so a not-yet-migrated producer that still sends `0` stays valid
  during the rollout; the new `vulnFinding.port` is strictly 1..65535 because it is brand new.
- A2: `confidence` and `patch_state` are absent-only (never `null`), because nmap-service skips them when `None`.

## Checklist

| ID | Task | Done |
|---|---|---|
| FP-01 | Feature document | [ ] |
| FP-02 | RED: failing tests (vulnFinding with/without port, port 0/70000, bad protocol, confidence/patch_state; finding.critical null port/protocol) | [ ] |
| FP-03 | GREEN: schemas + example | [ ] |
| FP-04 | Docs: `contracts/README.md`, D-15 | [ ] |
| FP-05 | `make verify` green | [ ] |

## Compatibility note

Existing messages remain valid: every addition is optional or widens a type. Producers can adopt the fields in any
order relative to consumers.
