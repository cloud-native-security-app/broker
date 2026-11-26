//! MSG-08 parity: the expected topology in `broker_verification::topology`
//! matches what `infra` implements, `terraform/messaging.tf` (cloud) and
//! `scripts/localstack-init.sh` (local). Offline text checks on the sibling
//! checkout `../infra`: no Docker, no network. When the sibling is absent
//! (this repo cloned alone) the tests print a notice and pass; they cannot
//! gate anything without the other repo. They look for stable literals, not a
//! full HCL or shell parse, so a refactor of `infra` formatting may need an
//! update here.

use std::path::PathBuf;

use broker_verification::topology::{self, QueueSpec, MAX_RECEIVE_COUNT};

fn infra_file(relative: &str) -> Option<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../infra")
        .join(relative);
    match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(_) => {
            eprintln!(
                "SKIP: {} not found (sibling infra checkout absent)",
                path.display()
            );
            None
        }
    }
}

/// The `"<queue>" = { ... }` block of `local.scan_queues`.
fn terraform_block<'a>(tf: &'a str, queue: &str) -> &'a str {
    let start = tf
        .find(&format!("\"{queue}\" = {{"))
        .unwrap_or_else(|| panic!("messaging.tf must declare queue {queue}"));
    let rest = &tf[start..];
    let end = rest.find("\n    }").expect("end of queue block");
    &rest[..end]
}

/// The `resource "aws_sqs_queue" "<id>" { ... }` block whose `name` is
/// `queue`, for queues outside `local.scan_queues` (no SNS topic).
fn standalone_queue_block<'a>(tf: &'a str, queue: &str) -> &'a str {
    let name_line = format!("= \"{queue}\"");
    let start = tf
        .match_indices("resource \"aws_sqs_queue\"")
        .map(|(i, _)| i)
        .find(|&i| {
            let rest = &tf[i..];
            let end = rest.find("\n}\n").unwrap_or(rest.len());
            rest[..end]
                .lines()
                .any(|l| l.trim_start().starts_with("name") && l.ends_with(&name_line))
        })
        .unwrap_or_else(|| panic!("messaging.tf must declare an aws_sqs_queue named {queue}"));
    let rest = &tf[start..];
    &rest[..rest.find("\n}\n").expect("end of queue resource")]
}

/// Value after `<field> =` in a queue block, e.g. `3600` or `24 * 3600`.
fn field_value<'a>(block: &'a str, field: &str) -> &'a str {
    block
        .lines()
        .filter_map(|line| line.trim().strip_prefix(field))
        .find_map(|rest| rest.trim_start().strip_prefix('='))
        .map(str::trim)
        .unwrap_or_else(|| panic!("{field} missing in block"))
}

const DEFAULT_RETENTION_SECS: u32 = 4 * 24 * 3600;
const DEFAULT_DLQ_RETENTION_SECS: u32 = 14 * 24 * 3600;

/// A queue without an SNS topic is its own `aws_sqs_queue` block plus its
/// DLQ block: visibility, retention, SSE and redrive are checked there.
fn assert_standalone_queue_in_terraform(tf: &str, spec: &QueueSpec) {
    let dlq_name = spec.dlq_name();
    let block = standalone_queue_block(tf, spec.name);
    assert_eq!(
        field_value(block, "visibility_timeout_seconds")
            .split_whitespace()
            .next(),
        Some(spec.visibility_secs.to_string().as_str()),
        "{} visibility",
        spec.name
    );
    assert_eq!(spec.retention_secs, DEFAULT_RETENTION_SECS);
    assert_eq!(
        field_value(block, "message_retention_seconds"),
        "local.sqs_default_retention",
        "{} retention",
        spec.name
    );
    assert_eq!(
        field_value(block, "sqs_managed_sse_enabled"),
        "true",
        "{} SSE",
        spec.name
    );
    assert!(
        block.contains(&format!("maxReceiveCount     = {MAX_RECEIVE_COUNT}")),
        "{} maxReceiveCount",
        spec.name
    );

    let dlq = standalone_queue_block(tf, &dlq_name);
    assert!(
        block.contains(&format!(
            "deadLetterTargetArn = aws_sqs_queue.{}.arn",
            resource_id(tf, dlq)
        )),
        "{} redrive targets {dlq_name}",
        spec.name
    );
    assert_eq!(spec.dlq_retention_secs, DEFAULT_DLQ_RETENTION_SECS);
    assert_eq!(
        field_value(dlq, "message_retention_seconds"),
        "local.sqs_default_dlq_retention",
        "{dlq_name} retention"
    );
    assert_eq!(field_value(dlq, "sqs_managed_sse_enabled"), "true");
}

/// Terraform resource id of an `aws_sqs_queue` block found by `standalone_queue_block`.
fn resource_id<'a>(_tf: &'a str, block: &'a str) -> &'a str {
    let header = block.lines().next().expect("resource header");
    header.split('"').nth(3).expect("resource id")
}

fn assert_queue_in_terraform(tf: &str, spec: &QueueSpec) {
    let Some(topic) = spec.topic else {
        return assert_standalone_queue_in_terraform(tf, spec);
    };
    let block = terraform_block(tf, spec.name);
    assert!(
        block.contains(&format!("topic              = \"{}\"", topic))
            || block.contains(&format!("topic = \"{}\"", topic)),
        "{} topic in messaging.tf",
        spec.name
    );
    match spec.routing_key_prefix {
        Some(prefix) => assert!(
            block.contains(&format!(
                "{{ routing_key = [{{ prefix = \"{prefix}\" }}] }}"
            )),
            "{} filter policy in messaging.tf",
            spec.name
        ),
        None => assert!(
            block.contains("filter_policy = null") || block.contains("filter_policy      = null")
        ),
    }
    if spec.name == "ms-nmap-scan-requests" {
        assert!(
            block.contains(
                "max(var.ssh_connect_timeout_secs + var.ssh_command_timeout_secs, var.local_scan_timeout_secs) + var.nvd_enrichment_budget_secs + 60"
            ),
            "requests visibility formula (Q1)"
        );
        assert_eq!(spec.retention_secs, 4200);
        assert_eq!(field_value(block, "retention"), "4200");
        assert_eq!(spec.dlq_retention_secs, 24 * 3600);
        assert_eq!(field_value(block, "dlq_retention"), "24 * 3600");
    } else {
        assert!(block.contains(&format!("visibility_timeout = {}", spec.visibility_secs)));
        assert!(block.contains("local.sqs_default_retention"));
        assert!(block.contains("local.sqs_default_dlq_retention"));
    }
}

#[test]
fn messaging_tf_describes_the_expected_topology() {
    let Some(tf) = infra_file("terraform/messaging.tf") else {
        return;
    };
    for topic in topology::topics() {
        assert!(tf.contains(&format!("\"{topic}\"")), "topic {topic}");
    }
    for spec in topology::queues() {
        assert_queue_in_terraform(&tf, &spec);
    }
    assert!(
        tf.contains("4 * 24 * 3600"),
        "SQS default retention is 4 days"
    );
    assert!(
        tf.contains("14 * 24 * 3600"),
        "default DLQ retention is 14 days"
    );
    assert!(tf.contains(&format!("maxReceiveCount     = {MAX_RECEIVE_COUNT}")));
    let standalone = topology::queues()
        .iter()
        .filter(|q| q.topic.is_none())
        .count();
    assert_eq!(
        tf.matches("sqs_managed_sse_enabled   = true").count()
            + tf.matches("sqs_managed_sse_enabled    = true").count(),
        2 + 2 * standalone,
        "SSE-SQS on the consumer queues and the DLQs, shared and standalone"
    );
    assert!(
        tf.contains("long polling"),
        "long polling is on the consumer queues"
    );
    assert!(
        tf.contains("\"aws:SecureTransport\" = \"false\""),
        "HTTPS-only policy"
    );
    assert!(tf.contains("sns.amazonaws.com") && tf.contains("aws:SourceArn"));
}

#[test]
fn localstack_init_creates_the_same_names_filter_and_redrive() {
    let Some(init) = infra_file("scripts/localstack-init.sh") else {
        return;
    };
    for topic in topology::topics() {
        assert!(init.contains(topic), "topic {topic}");
    }
    assert!(init.contains(&format!("MAX_RECEIVE_COUNT={MAX_RECEIVE_COUNT}")));
    assert!(
        init.contains("REQUESTS_VISIBILITY=$((LONGEST_SCAN + NVD_BUDGET + 60))"),
        "requests visibility formula (Q1)"
    );
    for spec in topology::queues() {
        if spec.topic.is_none() {
            assert_standalone_queue_in_init(&init, &spec);
            continue;
        }
        // "queue|visibility" spec lines; the requests queue computes its own.
        assert!(init.contains(&format!("\"{}|", spec.name)), "{}", spec.name);
        if spec.name != "ms-nmap-scan-requests" {
            assert!(
                init.contains(&format!("\"{}|{}\"", spec.name, spec.visibility_secs)),
                "{} visibility",
                spec.name
            );
        }
        if let Some(filter) = spec.filter_policy() {
            assert!(init.contains(&filter), "{} filter", spec.name);
        }
    }
    assert!(init.contains("RawMessageDelivery"));
    assert!(
        init.contains("create_queue \"$queue-dlq\""),
        "every queue has a <name>-dlq"
    );
}

/// A queue without a topic is created directly with its visibility and DLQ,
/// and nothing subscribes it to SNS.
fn assert_standalone_queue_in_init(init: &str, spec: &QueueSpec) {
    let dlq_name = spec.dlq_name();
    assert!(
        init.lines()
            .any(|l| l.trim() == format!("create_queue \"{dlq_name}\"")),
        "{dlq_name} created"
    );
    assert!(
        init.lines().any(|l| l.trim()
            == format!(
                "create_queue \"{}\" {} \"{dlq_name}\"",
                spec.name, spec.visibility_secs
            )),
        "{} created with visibility and DLQ",
        spec.name
    );
    assert!(
        !init
            .lines()
            .any(|l| l.trim_start().starts_with("subscribe ") && l.contains(spec.name)),
        "{} must not be subscribed to SNS",
        spec.name
    );
}
