//! `idbseqd`: the idbseq HTTP API server.

pub mod app;
pub mod audit;
pub mod auth;
pub mod backup;
pub mod cli;
pub mod clock;
pub mod config;
pub mod db;
pub mod dto;
pub mod envelope;
pub mod error;
pub mod extract;
pub mod local;
pub mod metrics;
pub mod openapi;
pub mod routes;
pub mod serve;
pub mod state;
