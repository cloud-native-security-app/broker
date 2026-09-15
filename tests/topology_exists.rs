//! Verifica que la topología cargada en un `rabbitmq:management` real
//! coincide exactamente con `rabbitmq/definitions.json`: los 4 exchanges
//! (con sus DLX), las 6 colas (3 principales + 3 `.dlq`) y los 7 bindings
//! — incluidos los bindings diferenciados de `scan.outcomes` (routing keys
//! distintas para `ms-analisis.scan-outcomes` y `gateway.scan-outcomes`).
//!
//! Se consulta la API HTTP de management (autenticada como `lab-admin`,
//! nunca con un usuario de servicio — ver `docs/security-scope.md`) porque
//! expone `arguments`/`routing_key` exactos, cosa que una declaración AMQP
//! pasiva no siempre deja inspeccionar cómodamente (ver
//! `progress/explore_lapin_testcontainers.md` §3-4).

mod common;

use std::collections::BTreeSet;

use broker_verification::{lab_credentials, VHOST};
use serde_json::Value;

async fn get_json(client: &reqwest::Client, base: &str, path: &str) -> Value {
    client
        .get(format!("{base}{path}"))
        .basic_auth(
            lab_credentials::ADMIN_USER,
            Some(lab_credentials::ADMIN_PASSWORD),
        )
        .send()
        .await
        .expect("la API de management debe responder")
        .error_for_status()
        .expect("la API de management no debe devolver error")
        .json()
        .await
        .expect("la respuesta debe ser JSON válido")
}

fn names(values: &Value) -> BTreeSet<String> {
    values
        .as_array()
        .expect("se esperaba un array JSON")
        .iter()
        .map(|v| {
            v["name"]
                .as_str()
                .expect("cada elemento tiene name")
                .to_string()
        })
        .collect()
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn topology_matches_definitions_json_exactly() {
    let container = common::start_broker().await;
    let base = common::management_base_url(&container).await;
    let client = reqwest::Client::new();

    // --- Exchanges ---
    let exchanges = get_json(&client, &base, &format!("/exchanges/{VHOST}")).await;
    let exchange_names = names(&exchanges);
    for expected in [
        "scan.requests",
        "scan.requests.dlx",
        "scan.outcomes",
        "scan.outcomes.dlx",
    ] {
        assert!(
            exchange_names.contains(expected),
            "falta el exchange {expected}, encontrados: {exchange_names:?}"
        );
    }

    let exchange_type = |name: &str| -> String {
        exchanges
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["name"] == name)
            .unwrap_or_else(|| panic!("exchange {name} debe existir"))["type"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(exchange_type("scan.requests"), "topic");
    assert_eq!(exchange_type("scan.outcomes"), "topic");
    assert_eq!(exchange_type("scan.requests.dlx"), "direct");
    assert_eq!(exchange_type("scan.outcomes.dlx"), "direct");

    // --- Colas: existen exactamente las 6 esperadas (3 principales + 3 .dlq) ---
    let queues = get_json(&client, &base, &format!("/queues/{VHOST}")).await;
    let queue_names = names(&queues);
    let expected_queues: BTreeSet<String> = [
        "ms-nmap.scan-requests",
        "ms-nmap.scan-requests.dlq",
        "ms-analisis.scan-outcomes",
        "ms-analisis.scan-outcomes.dlq",
        "gateway.scan-outcomes",
        "gateway.scan-outcomes.dlq",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(
        queue_names, expected_queues,
        "las colas cargadas no coinciden exactamente con las de definitions.json"
    );

    // Cada cola principal tiene x-delivery-limit: 3 y su DLX/routing-key propios.
    let queue_arguments = |name: &str| -> Value {
        queues
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["name"] == name)
            .unwrap_or_else(|| panic!("cola {name} debe existir"))["arguments"]
            .clone()
    };

    let main_queues = [
        (
            "ms-nmap.scan-requests",
            "scan.requests.dlx",
            "ms-nmap.scan-requests.dead",
        ),
        (
            "ms-analisis.scan-outcomes",
            "scan.outcomes.dlx",
            "ms-analisis.scan-outcomes.dead",
        ),
        (
            "gateway.scan-outcomes",
            "scan.outcomes.dlx",
            "gateway.scan-outcomes.dead",
        ),
    ];
    for (queue, dlx, dead_routing_key) in main_queues {
        let args = queue_arguments(queue);
        assert_eq!(args["x-queue-type"], "quorum", "{queue} debe ser quorum");
        assert_eq!(
            args["x-delivery-limit"], 3,
            "{queue} debe limitar los reintentos a 3 (RNF-06, ver rabbitmq/README.md)"
        );
        assert_eq!(
            args["x-dead-letter-exchange"], dlx,
            "{queue} debe dead-letrar hacia {dlx}"
        );
        assert_eq!(
            args["x-dead-letter-routing-key"], dead_routing_key,
            "{queue} debe usar la routing key de dead-letter {dead_routing_key}"
        );
    }

    // --- Bindings: exactamente los 7 esperados, con las routing keys diferenciadas ---
    let bindings = get_json(&client, &base, &format!("/bindings/{VHOST}")).await;
    let binding_tuples: BTreeSet<(String, String, String)> = bindings
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| !b["source"].as_str().unwrap_or_default().is_empty())
        .map(|b| {
            (
                b["source"].as_str().unwrap().to_string(),
                b["destination"].as_str().unwrap().to_string(),
                b["routing_key"].as_str().unwrap().to_string(),
            )
        })
        .collect();

    let expected_bindings: BTreeSet<(String, String, String)> = [
        ("scan.requests", "ms-nmap.scan-requests", "scan.request"),
        (
            "scan.requests.dlx",
            "ms-nmap.scan-requests.dlq",
            "ms-nmap.scan-requests.dead",
        ),
        (
            "scan.outcomes",
            "ms-analisis.scan-outcomes",
            "scan.outcome.completed",
        ),
        (
            "scan.outcomes",
            "ms-analisis.scan-outcomes",
            "scan.outcome.failed",
        ),
        ("scan.outcomes", "gateway.scan-outcomes", "scan.outcome.#"),
        (
            "scan.outcomes.dlx",
            "ms-analisis.scan-outcomes.dlq",
            "ms-analisis.scan-outcomes.dead",
        ),
        (
            "scan.outcomes.dlx",
            "gateway.scan-outcomes.dlq",
            "gateway.scan-outcomes.dead",
        ),
    ]
    .into_iter()
    .map(|(s, d, rk)| (s.to_string(), d.to_string(), rk.to_string()))
    .collect();

    assert_eq!(
        binding_tuples, expected_bindings,
        "los bindings cargados no coinciden exactamente con definitions.json"
    );

    // ms-analisis.scan-outcomes NUNCA se bindea con scan.outcome.started.
    assert!(
        !binding_tuples.contains(&(
            "scan.outcomes".to_string(),
            "ms-analisis.scan-outcomes".to_string(),
            "scan.outcome.started".to_string()
        )),
        "ms-analisis.scan-outcomes no debe recibir scan.outcome.started"
    );
}
