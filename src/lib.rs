//! Crate de verificación de la topología y el contrato de mensajes del
//! Broker. No es una librería compartida ni produce un binario de
//! producción — ver `docs/architecture.md`.
//!
//! Expone dos módulos de datos puros: [`contracts`] (validación de payloads
//! contra los JSON Schema de `contracts/`) y [`topology`] (la topología
//! SNS + SQS esperada, fuente de verdad de nombres y parámetros; ver
//! `docs/decisions.md` D-13).

pub mod contracts;
pub mod topology;
