//! Shared LocalStack helpers for the SNS + SQS integration tests.
//!
//! Each test starts its own LocalStack container (no mocks, no AWS account)
//! and builds the topology from `broker_verification::topology`, mirroring
//! `infra/scripts/localstack-init.sh` plus the encryption and retention that
//! `infra/terraform/messaging.tf` applies and LocalStack init omits.

#![allow(dead_code)] // every test binary compiles this module but uses a subset

use std::collections::HashMap;
use std::time::{Duration, Instant};

use aws_sdk_sns::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_sns::types::MessageAttributeValue as SnsAttr;
use aws_sdk_sqs::types::{Message, QueueAttributeName};
use broker_verification::topology::{self, QueueSpec, MAX_RECEIVE_COUNT};
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage};

const LOCALSTACK_TAG: &str = "4.4";
const REGION: &str = "us-east-1";

pub struct Env {
    _container: ContainerAsync<GenericImage>,
    pub sns: aws_sdk_sns::Client,
    pub sqs: aws_sdk_sqs::Client,
    /// queue name -> queue URL (consumer queues and `-dlq` queues).
    pub queue_urls: HashMap<String, String>,
    /// topic name -> topic ARN.
    pub topic_arns: HashMap<String, String>,
}

/// Starts LocalStack and creates the full topology. `visibility_override`
/// replaces every consumer queue's visibility timeout (DLQ tests need 1 s).
pub async fn start(visibility_override: Option<u32>) -> Env {
    let container = GenericImage::new("localstack/localstack", LOCALSTACK_TAG)
        .with_exposed_port(4566.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready."))
        .start()
        .await
        .expect("the LocalStack test container must start");
    let port = container
        .get_host_port_ipv4(4566.tcp())
        .await
        .expect("LocalStack must publish port 4566");
    let endpoint = format!("http://127.0.0.1:{port}");
    let credentials = || Credentials::new("test", "test", None, None, "localstack-test");
    let sns = aws_sdk_sns::Client::from_conf(
        aws_sdk_sns::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(REGION))
            .endpoint_url(&endpoint)
            .credentials_provider(credentials())
            .build(),
    );
    let sqs = aws_sdk_sqs::Client::from_conf(
        aws_sdk_sqs::Config::builder()
            .behavior_version(aws_sdk_sqs::config::BehaviorVersion::latest())
            .region(aws_sdk_sqs::config::Region::new(REGION))
            .endpoint_url(&endpoint)
            .credentials_provider(credentials())
            .build(),
    );
    let mut env = Env {
        _container: container,
        sns,
        sqs,
        queue_urls: HashMap::new(),
        topic_arns: HashMap::new(),
    };
    env.create_topology(visibility_override).await;
    env
}

impl Env {
    async fn create_topology(&mut self, visibility_override: Option<u32>) {
        for topic in topology::topics() {
            let arn = self
                .sns
                .create_topic()
                .name(topic)
                .send()
                .await
                .expect("create topic")
                .topic_arn
                .expect("topic arn");
            self.topic_arns.insert(topic.to_string(), arn);
        }
        for spec in topology::queues() {
            self.create_queue_pair(&spec, visibility_override).await;
            self.subscribe(&spec).await;
        }
    }

    async fn create_queue_pair(&mut self, spec: &QueueSpec, visibility_override: Option<u32>) {
        let dlq_name = spec.dlq_name();
        let dlq_url = self
            .create_queue(
                &dlq_name,
                &[
                    (
                        QueueAttributeName::MessageRetentionPeriod,
                        spec.dlq_retention_secs.to_string(),
                    ),
                    (QueueAttributeName::SqsManagedSseEnabled, "true".into()),
                ],
            )
            .await;
        let dlq_arn = self.queue_arn(&dlq_url).await;
        let redrive = format!(
            r#"{{"deadLetterTargetArn":"{dlq_arn}","maxReceiveCount":"{MAX_RECEIVE_COUNT}"}}"#
        );
        let visibility = visibility_override.unwrap_or(spec.visibility_secs);
        let url = self
            .create_queue(
                spec.name,
                &[
                    (
                        QueueAttributeName::VisibilityTimeout,
                        visibility.to_string(),
                    ),
                    (
                        QueueAttributeName::MessageRetentionPeriod,
                        spec.retention_secs.to_string(),
                    ),
                    (QueueAttributeName::SqsManagedSseEnabled, "true".into()),
                    (QueueAttributeName::RedrivePolicy, redrive),
                ],
            )
            .await;
        self.queue_urls.insert(dlq_name, dlq_url);
        self.queue_urls.insert(spec.name.to_string(), url);
    }

    async fn create_queue(&self, name: &str, attrs: &[(QueueAttributeName, String)]) -> String {
        let mut request = self.sqs.create_queue().queue_name(name);
        for (key, value) in attrs {
            request = request.attributes(key.clone(), value);
        }
        request
            .send()
            .await
            .unwrap_or_else(|err| panic!("create queue {name}: {err:?}"))
            .queue_url
            .expect("queue url")
    }

    pub async fn queue_arn(&self, url: &str) -> String {
        self.sqs
            .get_queue_attributes()
            .queue_url(url)
            .attribute_names(QueueAttributeName::QueueArn)
            .send()
            .await
            .expect("queue attributes")
            .attributes
            .expect("attributes")
            .remove(&QueueAttributeName::QueueArn)
            .expect("queue arn")
    }

    async fn subscribe(&self, spec: &QueueSpec) {
        let queue_arn = self.queue_arn(&self.queue_urls[spec.name]).await;
        let subscription = self
            .sns
            .subscribe()
            .topic_arn(&self.topic_arns[spec.topic])
            .protocol("sqs")
            .endpoint(queue_arn)
            .send()
            .await
            .expect("subscribe")
            .subscription_arn
            .expect("subscription arn");
        self.set_subscription_attribute(&subscription, "RawMessageDelivery", "true")
            .await;
        if let Some(filter) = spec.filter_policy() {
            self.set_subscription_attribute(&subscription, "FilterPolicy", &filter)
                .await;
        }
    }

    async fn set_subscription_attribute(&self, subscription: &str, name: &str, value: &str) {
        self.sns
            .set_subscription_attributes()
            .subscription_arn(subscription)
            .attribute_name(name)
            .attribute_value(value)
            .send()
            .await
            .unwrap_or_else(|err| panic!("set {name}: {err:?}"));
    }

    /// Publishes `body` to `topic` with the `routing_key` String attribute.
    pub async fn publish(&self, topic: &str, routing_key: &str, body: &str) {
        let attribute = SnsAttr::builder()
            .data_type("String")
            .string_value(routing_key)
            .build()
            .expect("routing_key attribute");
        self.sns
            .publish()
            .topic_arn(&self.topic_arns[topic])
            .message(body)
            .message_attributes("routing_key", attribute)
            .send()
            .await
            .expect("publish");
    }

    /// Receives and deletes every message that shows up within `wait`.
    pub async fn drain(&self, queue: &str, wait: Duration) -> Vec<Message> {
        let url = &self.queue_urls[queue];
        let deadline = Instant::now() + wait;
        let mut messages = Vec::new();
        while Instant::now() < deadline {
            let received = self
                .sqs
                .receive_message()
                .queue_url(url)
                .message_attribute_names("All")
                .max_number_of_messages(10)
                .wait_time_seconds(1)
                .send()
                .await
                .expect("receive");
            for message in received.messages.unwrap_or_default() {
                self.sqs
                    .delete_message()
                    .queue_url(url)
                    .receipt_handle(message.receipt_handle.clone().expect("receipt handle"))
                    .send()
                    .await
                    .expect("delete");
                messages.push(message);
            }
            if !messages.is_empty() {
                break;
            }
        }
        messages
    }
}

/// `routing_key` of a raw-delivery message, taken from its SQS attributes.
pub fn routing_key(message: &Message) -> Option<String> {
    message
        .message_attributes
        .as_ref()?
        .get("routing_key")?
        .string_value
        .clone()
}
