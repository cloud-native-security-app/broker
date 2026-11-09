//! MSG-06: `RedrivePolicy.maxReceiveCount=3` moves a message that is received
//! three times without being deleted to the DLQ (the SQS replacement for the
//! AMQP `x-delivery-limit: 3`). Runs against LocalStack with a 1 s visibility
//! timeout so the receives do not take minutes.
//!
//! Malformed-message handling (Q5, explicit DLQ send) is consumer behavior
//! owned by `gateway` and `nmap-service`; it is not testable from this repo.

mod support;

use std::time::{Duration, Instant};

use broker_verification::topology;

const VISIBILITY_SECS: u32 = 1;
/// Literal on purpose: the signed-off value, independent of the topology constant.
const MAX_RECEIVE_COUNT: u32 = 3;
const BODY: &str = r#"{"correlation_id":"corr-dlq-1","requested_by":"analyst@example.test"}"#;

#[tokio::test]
#[ignore = "requiere Docker"]
async fn message_not_deleted_after_max_receive_count_lands_in_the_dlq() {
    let env = support::start(Some(VISIBILITY_SECS)).await;
    let queue = "ms-nmap-scan-cancellations";
    let queue_url = env.queue_urls[queue].clone();
    let dlq = topology::queues()
        .into_iter()
        .find(|q| q.name == queue)
        .expect("queue spec")
        .dlq_name();

    env.publish("scan-cancellations", "scan.cancellation", BODY)
        .await;

    // Receive without deleting: every visibility expiry counts as a failed attempt.
    for attempt in 1..=MAX_RECEIVE_COUNT {
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut received = None;
        while received.is_none() && Instant::now() < deadline {
            received = env
                .sqs
                .receive_message()
                .queue_url(&queue_url)
                .wait_time_seconds(1)
                .send()
                .await
                .expect("receive")
                .messages
                .unwrap_or_default()
                .pop();
        }
        let message = received.unwrap_or_else(|| panic!("receive #{attempt} must return it"));
        assert_eq!(message.body.as_deref(), Some(BODY));
        tokio::time::sleep(Duration::from_secs(VISIBILITY_SECS as u64 + 1)).await;
    }

    // The next receive attempt triggers the redrive; the DLQ then holds the message.
    let in_source = env
        .sqs
        .receive_message()
        .queue_url(&queue_url)
        .wait_time_seconds(1)
        .send()
        .await
        .expect("receive")
        .messages
        .unwrap_or_default();
    assert!(
        in_source.is_empty(),
        "after {MAX_RECEIVE_COUNT} receives the source queue must not redeliver"
    );
    let in_dlq = env.drain(&dlq, Duration::from_secs(15)).await;
    assert_eq!(in_dlq.len(), 1, "the message must be in {dlq}");
    assert_eq!(in_dlq[0].body.as_deref(), Some(BODY));
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn deleted_message_never_reaches_the_dlq() {
    let env = support::start(Some(VISIBILITY_SECS)).await;
    env.publish("scan-cancellations", "scan.cancellation", BODY)
        .await;

    let handled = env
        .drain("ms-nmap-scan-cancellations", Duration::from_secs(10))
        .await;
    assert_eq!(handled.len(), 1, "drain receives and deletes");

    tokio::time::sleep(Duration::from_secs(VISIBILITY_SECS as u64 * 4)).await;
    assert!(env
        .drain("ms-nmap-scan-cancellations-dlq", Duration::from_secs(2))
        .await
        .is_empty());
}
