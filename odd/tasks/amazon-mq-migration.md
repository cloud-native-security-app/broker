# Amazon MQ migration (broker repo)

Self-contained: any AI agent can start from this file with no prior context.

- Feature id: `amazon-mq-migration`
- Branch: `v1`
- Engram topic: `odd/amazon-mq-migration/tasks` (project: broker)
- Master plan: repo `infra`, path `odd/tasks/cloud-native-deploy.md`, Engram `odd/cloud-native-deploy/tasks` (project: infra)
- Dependency: infra task TF-10 creates the Amazon MQ broker; this repo only loads topology into it.

## Objective

Replace the self-managed RabbitMQ container that runs on ECS with **Amazon MQ for RabbitMQ**, keeping the
AMQP topology and message contract unchanged (vhost `security-app`, topic exchanges, quorum queues with
`x-delivery-limit: 3`, dead-letter exchanges, least-privilege per-service users).

## Why

No broker container to operate, patch or persist on ECS. Amazon MQ keeps AMQP, so `gateway` and
`nmap-service` keep their `lapin` clients. The topology in `rabbitmq/definitions.json` must remain the single
source of truth, both locally and in AWS.

## Scope

In:
- Adapt `rabbitmq/definitions.json` handling for Amazon MQ (users, passwords, vhost, policies).
- Script/job that imports definitions into Amazon MQ through the management API.
- CI workflow `ci.yml` running the verification suite.
- Make the integration tests runnable against an external endpoint via env vars.
- Keep the local image, `docker-compose.yml` and testcontainers flow for local development.

Out:
- Creating the broker itself (infra TF-10).
- Changes to service repos (CI-* tasks live there).
- Kafka or any other broker; Prometheus/alerting integration.

## Constraints

- AWS Academy Learner Lab: no `iam:CreateRole`, temporary credentials, `LabRole` only.
- Amazon MQ exposes **AMQPS on 5671 only** (no plaintext 5672); the management API is HTTPS on 443.
- Definitions cannot be baked into an image or loaded via `rabbitmq.conf`; they must be imported through the
  management API (`POST /api/definitions`) after the broker is up.
- Password hashes in `definitions.json` are lab-only; real passwords come from Secrets Manager / GitHub secrets
  and are never committed.
- Amazon MQ enforces password rules (length, no commas/colons) and may reject some policies or users.

## Tooling

- TDD: Strict. Source: user global config (Strict TDD Mode). Observe RED before implementing.
- Runner: `make verify` (fmt, clippy, `cargo test`, `cargo test -- --ignored`, `cargo doc`).
  Unit: `cargo test`. Integration (needs Docker): `cargo test -- --ignored`.
- Workflow lint: `actionlint`.
- Conventional Commits, no AI attribution; push/PR/merge are user decisions.

## Checklist

Spikes (record yes/no and fallback in Progress):
- [ ] MQ-01 Spike: Amazon MQ is allowed in the Learner Lab (single-instance vs cluster, instance types).
- [ ] MQ-02 Spike: quorum queues + `x-delivery-limit` work on the available engine version and deployment
      mode. Fallback: classic queues + DLX/TTL (see `docs/decisions.md`), or `CLUSTER_MULTI_AZ`.
- [ ] MQ-03 Spike: `POST /api/definitions` imports `rabbitmq/definitions.json` (or a derived copy) successfully.
- [ ] MQ-04 Spike: AMQPS on 5671 uses a public CA, so no lab CA is needed on AWS.

Implementation:
- [ ] MQ-05 Adapt definitions for Amazon MQ: inspect `rabbitmq/` and decide how users/password hashes are
      injected (render from env/secrets at import time), vhost handling, and any unsupported items.
- [ ] MQ-06 Import script/job (`scripts/import-definitions.sh` or workflow) using Learner Lab secrets; idempotent.
- [ ] MQ-07 CI workflow `.github/workflows/ci.yml` running fmt, clippy, unit tests and Docker integration tests.
- [ ] MQ-08 Tests (`topology_exists`, `tls`, `retry_delivery_limit`, `permissions`, `outcome_routing`,
      `cancellation`, `observability`, `message_contract`) runnable against an Amazon MQ endpoint via env vars
      (e.g. `BROKER_AMQPS_URL`, `BROKER_MGMT_URL`, credentials), defaulting to testcontainers.
- [ ] MQ-09 Document the AWS path in `README.md` and `docs/architecture.md`; keep local-dev instructions.

## Acceptance criteria

- Importing the definitions into Amazon MQ yields the same topology as local (verified by `topology_exists`).
- Permission and routing tests pass against Amazon MQ with per-service users.
- Retry/dead-letter behavior verified, or the fallback documented and tested.
- Local `make verify` still passes unchanged.
- No credentials committed.

## Checks

`make verify`; `actionlint` on workflows; import script run twice (idempotent); integration suite against Amazon MQ.

## Progress

- [x] Plan written (this document). Harness migration done in the same branch.
- [ ] MQ-01..MQ-09 pending (blocked on infra TF-10 for MQ-01..MQ-04 and MQ-08 against AWS).

## Next step

Run spikes MQ-01..MQ-04 once the infra repo can create a broker; meanwhile MQ-05 and MQ-07 can start.
