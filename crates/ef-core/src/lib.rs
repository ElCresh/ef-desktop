//! EcoFlow BLE protocol core.
//!
//! Transport, handshake, telemetry decoding and the async device runtime, forked from
//! the `ecoflow-ble-poc` validation harness.

pub mod auth;
pub mod ble;
pub mod cloud;
pub mod command;
pub mod crc;
pub mod crypto;
pub mod encpacket;
pub mod keydata;
pub mod manager;
pub mod model;
pub mod packet;
pub mod profile;
pub mod river;
pub mod secp160r1;
pub mod session;
