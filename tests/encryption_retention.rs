//! MSG-07: the Q4 decision (SSE-SQS, 1 h retention on requests and 24 h on
//! their DLQ, SQS defaults elsewhere) and the redrive/visibility/subscription
//! parameters are present on the topology the tests create. LocalStack does
//! not enforce them (and reports SSE-SQS as on by default, so the SSE check
//! is a guard, not a discriminator; the declaration itself is covered by the
//! `messaging.tf` parity), so this asserts that the attributes are stored as the
//! topology declares; enforcement is AWS's job. Resource policies are
//! compared with `infra` in `tests/topology_parity.rs`.

mod support;

use std::collections::HashMap;

use aws_sdk_sqs::types::QueueAttributeName;
use broker_verification::topology::{self, MAX_RECEIVE_COUNT};
use serde_json::Value;

async fn attributes(env: &support::Env, queue: &str) -> HashMap<QueueAttributeName, String> {
    env.sqs
        .get_queue_attributes()
        .queue_url(&env.queue_urls[queue])
        .attribute_names(QueueAttributeName::All)
        .send()
        .await
        .expect("queue attributes")
        .attributes
        .expect("attributes")
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn queues_carry_sse_retention_visibility_and_redrive_from_the_q4_decision() {
    let env = support::start(None).await;

    for spec in topology::queues() {
        let main = attributes(&env, spec.name).await;
        let dlq_name = spec.dlq_name();
        let dlq = attributes(&env, &dlq_name).await;

        for (name, attrs) in [(spec.name.to_string(), &main), (dlq_name.clone(), &dlq)] {
            assert_eq!(
                attrs
                    .get(&QueueAttributeName::SqsManagedSseEnabled)
                    .map(String::as_str),
                Some("true"),
                "{name} must use SSE-SQS"
            );
            assert!(
                !attrs.contains_key(&QueueAttributeName::KmsMasterKeyId),
                "{name}: Q4 chose AWS-managed keys, no KMS"
            );
        }
        assert_eq!(
            main[&QueueAttributeName::MessageRetentionPeriod],
            spec.retention_secs.to_string(),
            "{} retention",
            spec.name
        );
        assert_eq!(
            dlq[&QueueAttributeName::MessageRetentionPeriod],
            spec.dlq_retention_secs.to_string(),
            "{dlq_name} retention"
        );
        assert_eq!(
            main[&QueueAttributeName::VisibilityTimeout],
            spec.visibility_secs.to_string(),
            "{} visibility",
            spec.name
        );

        let redrive: Value =
            serde_json::from_str(&main[&QueueAttributeName::RedrivePolicy]).expect("redrive json");
        assert_eq!(
            redrive["deadLetterTargetArn"].as_str(),
            Some(env.queue_arn(&env.queue_urls[&dlq_name]).await.as_str())
        );
        assert_eq!(
            redrive["maxReceiveCount"].to_string().trim_matches('"'),
            MAX_RECEIVE_COUNT.to_string()
        );
    }
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn subscriptions_use_raw_delivery_and_only_the_gateway_one_is_filtered() {
    let env = support::start(None).await;

    for spec in topology::queues() {
        let queue_arn = env.queue_arn(&env.queue_urls[spec.name]).await;
        let subscriptions = env
            .sns
            .list_subscriptions_by_topic()
            .topic_arn(&env.topic_arns[spec.topic])
            .send()
            .await
            .expect("subscriptions")
            .subscriptions
            .unwrap_or_default();
        let subscription = subscriptions
            .iter()
            .find(|s| s.endpoint.as_deref() == Some(queue_arn.as_str()))
            .unwrap_or_else(|| panic!("{} must subscribe to {}", spec.name, spec.topic));
        let attrs = env
            .sns
            .get_subscription_attributes()
            .subscription_arn(subscription.subscription_arn.as_deref().expect("arn"))
            .send()
            .await
            .expect("subscription attributes")
            .attributes
            .expect("attributes");

        assert_eq!(
            attrs.get("RawMessageDelivery").map(String::as_str),
            Some("true")
        );
        let filter: Option<Value> = attrs
            .get("FilterPolicy")
            .map(|f| serde_json::from_str(f).expect("filter json"));
        let expected: Option<Value> = spec
            .filter_policy()
            .map(|f| serde_json::from_str(&f).expect("expected json"));
        assert_eq!(filter, expected, "{} filter policy", spec.name);
    }
}
