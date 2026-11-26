//! Expected SNS + SQS topology (MSG-01, `odd/tasks/sqs-sns-migration.md`).
//!
//! Pure data: the LocalStack integration tests build the topology from it,
//! and the parity test compares it with `infra` (`terraform/messaging.tf`,
//! `scripts/localstack-init.sh`). No AWS calls here.

/// SNS topics, one per flow.
pub fn topics() -> Vec<&'static str> {
    vec!["scan-requests", "scan-cancellations", "scan-outcomes"]
}

/// Consumer queue parameters. The DLQ is `<name>-dlq`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueSpec {
    pub name: &'static str,
    /// SNS topic the queue subscribes to; `None` when the producer sends to
    /// the queue directly (no subscription).
    pub topic: Option<&'static str>,
    /// `routing_key` prefix the subscription filters on, when filtered.
    pub routing_key_prefix: Option<&'static str>,
    pub visibility_secs: u32,
    pub retention_secs: u32,
    pub dlq_retention_secs: u32,
}

impl QueueSpec {
    pub fn dlq_name(&self) -> String {
        format!("{}-dlq", self.name)
    }

    /// SNS `FilterPolicy` JSON, `None` when the subscription is unfiltered.
    pub fn filter_policy(&self) -> Option<String> {
        self.routing_key_prefix
            .map(|prefix| format!(r#"{{"routing_key":[{{"prefix":"{prefix}"}}]}}"#))
    }
}

/// `RedrivePolicy.maxReceiveCount` shared by every consumer queue.
pub const MAX_RECEIVE_COUNT: u32 = 3;

/// Visibility timeout of `ms-nmap-scan-requests` (Q1): the longest scan (SSH
/// timeouts or the local network-mode nmap timeout), plus the NVD enrichment
/// budget, plus a fixed margin. Mirrors `infra/terraform/messaging.tf`.
pub fn requests_visibility_secs(
    ssh_connect_secs: u32,
    ssh_command_secs: u32,
    local_scan_secs: u32,
    nvd_budget_secs: u32,
) -> u32 {
    (ssh_connect_secs + ssh_command_secs).max(local_scan_secs) + nvd_budget_secs + 60
}

/// The consumer queues with infra's default timeouts (SSH 10 s + 900 s,
/// local scan 900 s, NVD budget 300 s).
pub fn queues() -> Vec<QueueSpec> {
    const DEFAULT_RETENTION: u32 = 4 * 24 * 3600;
    const DEFAULT_DLQ_RETENTION: u32 = 14 * 24 * 3600;
    vec![
        QueueSpec {
            name: "ms-nmap-scan-requests",
            topic: Some("scan-requests"),
            routing_key_prefix: None,
            visibility_secs: requests_visibility_secs(10, 900, 900, 300),
            // Requests carry SSH credentials: keep them briefly (Q4), but long
            // enough for MAX_RECEIVE_COUNT visibility cycles (3 x 1270 s) to
            // redrive a failing request to the DLQ before it expires.
            retention_secs: 4200,
            dlq_retention_secs: 24 * 3600,
        },
        QueueSpec {
            name: "ms-nmap-scan-cancellations",
            topic: Some("scan-cancellations"),
            routing_key_prefix: None,
            visibility_secs: 30,
            retention_secs: DEFAULT_RETENTION,
            dlq_retention_secs: DEFAULT_DLQ_RETENTION,
        },
        QueueSpec {
            name: "gateway-scan-outcomes",
            topic: Some("scan-outcomes"),
            routing_key_prefix: Some("scan.outcome."),
            visibility_secs: 30,
            retention_secs: DEFAULT_RETENTION,
            dlq_retention_secs: DEFAULT_DLQ_RETENTION,
        },
        // RF-20: user-service sends and consumes it directly (no SNS topic);
        // SQS drives the retries, one SMTP attempt per receive.
        QueueSpec {
            name: "user-service-report-emails",
            topic: None,
            routing_key_prefix: None,
            visibility_secs: 120,
            retention_secs: DEFAULT_RETENTION,
            dlq_retention_secs: DEFAULT_DLQ_RETENTION,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_are_one_per_flow() {
        assert_eq!(
            topics(),
            vec!["scan-requests", "scan-cancellations", "scan-outcomes"]
        );
    }

    #[test]
    fn requests_visibility_covers_the_longest_scan_plus_nvd_budget_and_margin() {
        assert_eq!(requests_visibility_secs(10, 900, 900, 300), 1270);
        assert_eq!(requests_visibility_secs(1, 1, 1, 0), 62);
        assert_eq!(requests_visibility_secs(10, 100, 600, 0), 660);
    }

    #[test]
    fn every_queue_has_a_dlq_and_a_topic_is_a_known_one_when_present() {
        let queues = queues();
        assert_eq!(queues.len(), 4);
        for queue in &queues {
            if let Some(topic) = queue.topic {
                assert!(topics().contains(&topic), "{queue:?}");
            }
            assert_eq!(queue.dlq_name(), format!("{}-dlq", queue.name));
        }
    }

    #[test]
    fn only_the_report_email_queue_has_no_topic() {
        let topicless: Vec<_> = queues()
            .into_iter()
            .filter(|q| q.topic.is_none())
            .map(|q| q.name)
            .collect();
        assert_eq!(topicless, vec!["user-service-report-emails"]);
    }

    #[test]
    fn requests_queue_keeps_ssh_credentials_for_70_minutes_and_dlq_one_day() {
        let queues = queues();
        let requests = queues
            .iter()
            .find(|q| q.name == "ms-nmap-scan-requests")
            .expect("requests queue");
        assert_eq!(requests.retention_secs, 4200);
        assert_eq!(requests.dlq_retention_secs, 24 * 3600);
        assert_eq!(requests.visibility_secs, 1270);
    }

    #[test]
    fn every_queue_retains_a_failing_message_until_it_is_redriven() {
        for queue in queues() {
            assert!(
                queue.retention_secs > MAX_RECEIVE_COUNT * queue.visibility_secs,
                "{}: retention {} s must exceed {} receives x {} s visibility",
                queue.name,
                queue.retention_secs,
                MAX_RECEIVE_COUNT,
                queue.visibility_secs
            );
        }
    }

    #[test]
    fn other_queues_use_sqs_default_retention() {
        for queue in queues()
            .iter()
            .filter(|q| q.name != "ms-nmap-scan-requests")
        {
            assert_eq!(queue.retention_secs, 4 * 24 * 3600, "{}", queue.name);
            assert_eq!(queue.dlq_retention_secs, 14 * 24 * 3600, "{}", queue.name);
            let visibility = if queue.topic.is_some() { 30 } else { 120 };
            assert_eq!(queue.visibility_secs, visibility, "{}", queue.name);
        }
    }

    #[test]
    fn only_gateway_outcomes_is_filtered_on_the_outcome_prefix() {
        for queue in queues() {
            let expected = (queue.name == "gateway-scan-outcomes")
                .then(|| r#"{"routing_key":[{"prefix":"scan.outcome."}]}"#.to_string());
            assert_eq!(queue.filter_policy(), expected, "{}", queue.name);
        }
    }
}
