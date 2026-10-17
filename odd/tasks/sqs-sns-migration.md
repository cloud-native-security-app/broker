# SQS + SNS migration (broker repo)

Self-contained: any AI agent can start from this file with no prior context.

- Feature id: `sqs-sns-migration`
- Branch: `v1`
- Engram topic: `odd/sqs-sns-migration/tasks` (project: broker). Mirror pending (Engram not written when this doc was created).
- Master plan: repo `infra`, path `odd/tasks/cloud-native-deploy.md`
- Supersedes: repo `broker`, path `odd/tasks/amazon-mq-migration.md` (Amazon MQ denied in the Learner Lab; its spike results are kept there).
- Dependent features: repo `gateway`, path `odd/tasks/sqs-sns-adapter.md`; repo `nmap-service`, path `odd/tasks/sqs-sns-adapter.md`; repo `infra`, path `odd/tasks/terraform-infra.md` (TF-10).
- Decision date: 2026-10-17 (user decision).

## Objective

Replace RabbitMQ (self-managed or Amazon MQ) with AWS-managed **SNS + SQS** for the cloud-native deploy,
to demonstrate a cloud-native architecture on managed services. The `broker` repo stops being a broker
and keeps only what is still valuable: the JSON Schema message contracts and the topology design (which
Terraform in `infra` implements).

## Problem

- Amazon MQ is denied in the Learner Lab (`mq:*` returns AccessDenied, see the superseded doc, MQ-01).
- The current topology (`rabbitmq/definitions.json`) uses AMQP concepts (topic exchanges, routing keys,
  quorum queues, `x-delivery-limit`, DLX, per-service users) that must be mapped to SNS/SQS.
- SQS/SNS verified in us-east-1 (2026-10-17): queue, DLQ via `RedrivePolicy.maxReceiveCount`, topic, queue
  policy, SNS->SQS subscription with `FilterPolicy` + `RawMessageDelivery`, publish; the filter dropped the
  non-matching event. Not yet verified: `LabRole` SQS/SNS access from inside a running ECS task.

## Why

Managed, serverless messaging: no broker to run, patch or persist; Terraform-managed topology; fits the
course goal (cloud-native on managed services).

## Current topology (source: `rabbitmq/definitions.json`, `gateway/src/broker.rs`, `nmap-service/src/messaging/rabbitmq.rs`)

Vhost `security-app`. Three topic exchanges, each with a direct DLX. Four consumer queues plus DLQs, all
quorum with `x-delivery-limit: 3`. Malformed messages are `nack(requeue=false)` and so go straight to the DLQ.

Flow: Gateway -> `scan.requests` -> `ms-nmap`; Gateway -> `scan.cancellations` -> `ms-nmap`;
`ms-nmap` -> `scan.outcomes` -> Gateway (`scan.outcome.#`) and `ms-analisis` (`completed`, `failed` only).
`ms-analisis` has no implementation in this workspace; its queue exists in the topology only.

## Proposed SQS/SNS topology (status: **Proposed**, needs sign-off in MSG-01)

Naming keeps the current queue names. SNS topics replace topic exchanges; the AMQP routing key becomes the
String message attribute `routing_key` (same values as today). All subscriptions use `RawMessageDelivery=true`
so the SQS body is the bare JSON contract and the attributes arrive as SQS message attributes.

| RabbitMQ element | SNS / SQS element | Settings |
|---|---|---|
| exchange `scan.requests` (topic) | SNS topic `scan-requests` | attribute `routing_key=scan.request` set by the publisher |
| queue `ms-nmap.scan-requests` + binding `scan.request` | SQS queue `ms-nmap-scan-requests` + subscription to `scan-requests`, raw delivery, no filter (single routing key) | `RedrivePolicy.maxReceiveCount=3`; visibility timeout >= max scan duration (open Q1); retention and SSE per open Q4 |
| DLX `scan.requests.dlx` + queue `ms-nmap.scan-requests.dlq` | SQS queue `ms-nmap-scan-requests-dlq` (redrive target; no DLX/binding needed) | `RedriveAllowPolicy` byQueue; retention 14 days |
| exchange `scan.cancellations` (topic) | SNS topic `scan-cancellations` | attribute `routing_key=scan.cancellation` |
| queue `ms-nmap.scan-cancellations` + binding `scan.cancellation` | SQS queue `ms-nmap-scan-cancellations` + subscription, raw delivery | `maxReceiveCount=3`; short visibility timeout (cancellation handling is quick) |
| DLX `scan.cancellations.dlx` + `ms-nmap.scan-cancellations.dlq` | SQS queue `ms-nmap-scan-cancellations-dlq` | as above |
| exchange `scan.outcomes` (topic) | SNS topic `scan-outcomes` | attribute `routing_key` in `scan.outcome.started\|completed\|failed` |
| queue `gateway.scan-outcomes` + binding `scan.outcome.#` | SQS queue `gateway-scan-outcomes` + subscription with `FilterPolicy {"routing_key":[{"prefix":"scan.outcome."}]}` | `maxReceiveCount=3` |
| queue `ms-analisis.scan-outcomes` + bindings `scan.outcome.completed`, `scan.outcome.failed` | SQS queue `ms-analisis-scan-outcomes` + subscription with `FilterPolicy {"routing_key":["scan.outcome.completed","scan.outcome.failed"]}` | `maxReceiveCount=3`; `started` is dropped by the filter, as today |
| DLX `scan.outcomes.dlx` + `gateway.scan-outcomes.dlq`, `ms-analisis.scan-outcomes.dlq` | SQS queues `gateway-scan-outcomes-dlq`, `ms-analisis-scan-outcomes-dlq` | two DLQs, one per consumer queue (same as today) |
| `x-delivery-limit: 3` (quorum) | `RedrivePolicy.maxReceiveCount=3` | counts receives, not requeues; a visibility-timeout expiry also counts |
| `nack(requeue=false)` on malformed | consumer leaves the message undeleted (or sets visibility to 0) and relies on `maxReceiveCount` | or sends it to the DLQ explicitly (open Q5) |
| per-service users and permissions | **lost**: all ECS tasks use `LabRole` | document as an accepted tradeoff; queue/topic policies still restrict the SNS->SQS path to the matching topic ARN |
| TLS / lab CA / AMQPS | HTTPS to public AWS endpoints (public CA) | the lab CA baked into `nmap-service` is no longer needed |

Queue policies: each SQS queue allows `sqs:SendMessage` only from `sns.amazonaws.com` with
`aws:SourceArn` equal to the topic it subscribes to.

### Semantics that differ from RabbitMQ (design constraints)

- **At-least-once delivery.** Standard SQS and SNS can deliver duplicates; consumers must be idempotent
  (dedupe on `scan_id`, to be confirmed against the contract). RabbitMQ quorum queues also redeliver, but
  the Gateway currently acks before handling; that must be revisited (delete only after a successful handle).
- **No ordering on standard queues.** Today ordering between a request and its cancellation is not guaranteed
  either (separate queues). FIFO topics/queues are possible (filter policies and redrive work, 300 msg/s per
  API action by default, 5-minute dedup window, topic and queue names end in `.fifo`) but add `MessageGroupId`
  design; proposal is standard queues (open Q3).
- **256 KB message limit** (body plus attributes). `ScanRequest` is tiny. `ScanOutcome` may carry scan results;
  its maximum size must be checked against the schema (open Q2). No extended-client (S3) pattern is planned.
- **Visibility timeout replaces unacked-message handling.** A scan can run for minutes; the consumer must either
  set a visibility timeout above the maximum scan time or extend it (`ChangeMessageVisibility`) while working.
  A timeout shorter than the scan causes duplicate scans and eventual DLQ routing.
- **Pull model.** Consumers long-poll (`WaitTimeSeconds=20`); there is no push channel and no consumer prefetch.
  No consumer-count limits across tasks: two `nmap-service` tasks compete for messages naturally.
- **Sensitive payload.** `scan.requests` carries real SSH credentials (`ssh_credentials_ref`, see
  `docs/security-scope.md`). Required: encryption at rest (SSE-SQS at minimum; KMS only if the Learner Lab allows
  it), HTTPS-only queue/topic policy (`aws:SecureTransport`), short retention on the requests queue and DLQ
  (open Q4), and no logging of message bodies. DLQ copies of requests persist credentials until retention expires.
- **No broker-side TTL per message needed today** (the current topology declares none).

### Open design questions (need a user decision; not decided here)

1. ~~Visibility timeout for `ms-nmap-scan-requests`: fixed ceiling or heartbeat extension?~~ **Decided 2026-10-17: fixed timeout**, computed in Terraform as `MS_NMAP_SSH_CONNECT_TIMEOUT_SECS + MS_NMAP_SSH_COMMAND_TIMEOUT_SECS + margin` (no `ChangeMessageVisibility` heartbeat).
2. ~~Maximum `ScanOutcome` size: 256 KB safe or S3 reference?~~ **Decided 2026-10-17: inline with a size guard.** One IP per scan, but the ports/vulnerabilities arrays are unbounded in the schema. `nmap-service` checks the serialized outcome (threshold about 250 KB); above it, it publishes a `failed` outcome with an explicit "result too large" reason. No S3 claim-check.
3. Standard queues (proposed) or FIFO for requests/cancellations?
4. Retention and encryption: SSE-SQS vs KMS, and retention for the requests queue and its DLQ (credentials inside).
5. Poison messages: wait for `maxReceiveCount` (3 visibility cycles) or send malformed messages to the DLQ immediately?
6. Requests and cancellations through SNS topics (proposed, uniform and extensible) or published straight to the SQS queue (one fewer hop, no fan-out)?
7. Keep the unused `ms-analisis` queues and DLQ in Terraform even though that service has no code here?
8. Is `scan.outcome.started` still published by `ms-nmap` (contract says pending)? The filters handle either answer.

## Scope

In:
- Topology design sign-off and the filter/attribute contract (this repo owns the design; `infra` implements it in Terraform).
- LocalStack setup (compose service and init) to run the topology locally.
- Reuse of `contracts/*.schema.json` as message bodies, with a contract test.
- Port of the integration tests from testcontainers RabbitMQ to LocalStack (real containers, no mocks).
- Retirement plan for the repo's broker role.

Out:
- Terraform resources (infra TF-10).
- Rewriting the Rust adapters (`gateway` and `nmap-service` own `odd/tasks/sqs-sns-adapter.md`).
- Gateway architecture redesign (still deferred).

## Constraints

- AWS Academy Learner Lab: no `iam:CreateRole`; every ECS task uses the single shared `LabRole`. Per-service least
  privilege is lost (accepted tradeoff, 2026-10-17). Mitigation: resource-level policies on queues and topics, and one
  topic/queue per flow.
- Temporary credentials; region `us-east-1`.
- `LabRole` access to SQS/SNS from ECS is unverified until MSG-02.
- Message bodies stay the existing JSON contracts; no contract changes in this feature.
- Artifacts in English; existing Spanish docs keep being edited in Spanish.

## Tooling

- TDD: Strict (source: user global config). Observe RED before implementing.
- Runner: `cargo test`; integration tests (Docker, LocalStack) with `cargo test -- --ignored`; `make verify` aggregate.
- Planning heuristic only: about 400 authored changed lines per task; not a cap.
- Conventional Commits, no AI attribution; push/PR/merge are user decisions.

## Checklist

| ID | Task | Done |
|---|---|---|
| MSG-01 | Topology design sign-off: resolve open questions Q1-Q8, then freeze the table above as the `infra` TF-10 contract | [ ] |
| MSG-02 | Spike: `LabRole` can use SQS/SNS from inside a running ECS Fargate task (publish, receive, delete, change visibility, redrive; with and without SSE) and which SSE option works; record yes/no and fallback | [ ] |
| MSG-03 | LocalStack local setup: `docker-compose.yml` service, init script creating the full topology (`awslocal`), README local-dev section; RED: a test that fails until the topology exists | [ ] |
| MSG-04 | Contract reuse: `contracts/*.schema.json` validate the SQS bodies; contract test publishes through SNS and validates the received body (adapt `message_contract.rs`) | [ ] |
| MSG-05 | Message attributes and filtering: define the `routing_key` attribute contract; tests that `ms-analisis` never receives `started`, Gateway receives all three, requests and cancellations reach only their queue (adapt `outcome_routing.rs`, `cancellation.rs`) | [ ] |
| MSG-06 | Retry and DLQ tests on LocalStack: `maxReceiveCount=3` moves the message to the DLQ; malformed-message handling per Q5 (adapt `retry_delivery_limit.rs`, `topology_exists.rs`) | [ ] |
| MSG-07 | Encryption and retention: apply the Q4 decision to the design and tests (`aws:SecureTransport` policy, SSE attributes present, retention values) | [ ] |
| MSG-08 | Hand-off to `infra`: parameter table (names, timeouts, retention, filters, policies) consumed by TF-10, plus a parity check that LocalStack init and Terraform describe the same topology | [ ] |
| MSG-09 | Broker repo retirement: remove `rabbitmq/`, AMQP test code and `lapin`/testcontainers-rabbitmq dev-dependencies; keep `contracts/`, `docs/security-scope.md` and the topology design; update `README.md`, `docs/architecture.md`, `docs/decisions.md` (new decision: SQS/SNS) | [ ] |

Tests with no SQS/SNS equivalent (`permissions.rs`: per-user ACLs; `tls.rs`: AMQPS/lab CA; `observability.rs`: RabbitMQ
metrics) are retired in MSG-09, or replaced by queue-policy and CloudWatch-metric checks if cheap.

## Acceptance criteria

- The topology table is signed off and every RabbitMQ element has a documented SNS/SQS counterpart.
- LocalStack topology tests pass: routing/filters, DLQ after 3 receives, contract bodies valid.
- MSG-02 result recorded; if `LabRole` is denied from ECS, the fallback decision is recorded before any Terraform apply.
- Parity check between LocalStack init and the `infra` Terraform passes.
- `make verify` passes; no credentials committed.

## Checks

`cargo test`; `cargo test -- --ignored` (LocalStack in Docker); `make verify`.

## Progress

- [x] Plan written (2026-10-17). SQS/SNS availability probe recorded (see the superseded doc, MQ-01).
- [ ] MSG-01..MSG-09 pending.

## Next step

MSG-01: get the user's decisions on open questions Q1-Q8; in parallel, MSG-02 (LabRole from ECS) can run as soon as
`infra` has a cluster and a one-off task.
