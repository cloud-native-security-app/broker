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
    pub topic: &'static str,
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

/// Visibility timeout of `ms-nmap-scan-requests` (Q1): a scan never outlives
/// its SSH timeouts, plus a fixed margin.
pub fn requests_visibility_secs(ssh_connect_secs: u32, ssh_command_secs: u32) -> u32 {
    ssh_connect_secs + ssh_command_secs + 60
}

/// The three consumer queues with infra's default SSH timeouts (10 s, 900 s).
pub fn queues() -> Vec<QueueSpec> {
    const DEFAULT_RETENTION: u32 = 4 * 24 * 3600;
    const DEFAULT_DLQ_RETENTION: u32 = 14 * 24 * 3600;
    vec![
        QueueSpec {
            name: "ms-nmap-scan-requests",
            topic: "scan-requests",
            routing_key_prefix: None,
            visibility_secs: requests_visibility_secs(10, 900),
            // Requests carry SSH credentials: keep them briefly (Q4).
            retention_secs: 3600,
            dlq_retention_secs: 24 * 3600,
        },
        QueueSpec {
            name: "ms-nmap-scan-cancellations",
            topic: "scan-cancellations",
            routing_key_prefix: None,
            visibility_secs: 30,
            retention_secs: DEFAULT_RETENTION,
            dlq_retention_secs: DEFAULT_DLQ_RETENTION,
        },
        QueueSpec {
            name: "gateway-scan-outcomes",
            topic: "scan-outcomes",
            routing_key_prefix: Some("scan.outcome."),
            visibility_secs: 30,
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
    fn requests_visibility_covers_both_ssh_timeouts_plus_margin() {
        assert_eq!(requests_visibility_secs(10, 900), 970);
        assert_eq!(requests_visibility_secs(1, 1), 62);
    }

    #[test]
    fn every_queue_subscribes_to_a_known_topic_and_has_a_dlq() {
        let queues = queues();
        assert_eq!(queues.len(), 3);
        for queue in &queues {
            assert!(topics().contains(&queue.topic), "{queue:?}");
            assert_eq!(queue.dlq_name(), format!("{}-dlq", queue.name));
        }
    }

    #[test]
    fn requests_queue_keeps_ssh_credentials_for_at_most_one_hour_and_dlq_one_day() {
        let queues = queues();
        let requests = queues
            .iter()
            .find(|q| q.name == "ms-nmap-scan-requests")
            .expect("requests queue");
        assert_eq!(requests.retention_secs, 3600);
        assert_eq!(requests.dlq_retention_secs, 24 * 3600);
        assert_eq!(requests.visibility_secs, 970);
    }

    #[test]
    fn other_queues_use_sqs_default_retention() {
        for queue in queues()
            .iter()
            .filter(|q| q.name != "ms-nmap-scan-requests")
        {
            assert_eq!(queue.retention_secs, 4 * 24 * 3600, "{}", queue.name);
            assert_eq!(queue.dlq_retention_secs, 14 * 24 * 3600, "{}", queue.name);
            assert_eq!(queue.visibility_secs, 30, "{}", queue.name);
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
