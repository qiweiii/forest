// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod db;

use crate::db::DBStatistics;
use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use prometheus::core::{AtomicU64, GenericCounterVec, Opts};
use prometheus::{Encoder, TextEncoder};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::warn;

pub static DEFAULT_REGISTRY: Lazy<RwLock<prometheus_client::registry::Registry>> =
    Lazy::new(Default::default);

pub static LRU_CACHE_HIT: Lazy<Box<GenericCounterVec<AtomicU64>>> = Lazy::new(|| {
    let lru_cache_hit = Box::new(
        GenericCounterVec::<AtomicU64>::new(
            Opts::new("lru_cache_hit", "Stats of lru cache hit"),
            &[labels::KIND],
        )
        .expect("Defining the lru_cache_hit metric must succeed"),
    );
    prometheus::default_registry()
        .register(lru_cache_hit.clone())
        .expect("Registering the lru_cache_hit metric with the metrics registry must succeed");
    lru_cache_hit
});
pub static LRU_CACHE_MISS: Lazy<Box<GenericCounterVec<AtomicU64>>> = Lazy::new(|| {
    let lru_cache_miss = Box::new(
        GenericCounterVec::<AtomicU64>::new(
            Opts::new("lru_cache_miss", "Stats of lru cache miss"),
            &[labels::KIND],
        )
        .expect("Defining the lru_cache_miss metric must succeed"),
    );
    prometheus::default_registry()
        .register(lru_cache_miss.clone())
        .expect("Registering the lru_cache_miss metric with the metrics registry must succeed");
    lru_cache_miss
});

pub async fn init_prometheus<DB>(
    prometheus_listener: TcpListener,
    db_directory: PathBuf,
    db: Arc<DB>,
) -> anyhow::Result<()>
where
    DB: DBStatistics + Send + Sync + 'static,
{
    let registry = prometheus::default_registry();

    // Add the DBCollector to the registry
    let db_collector = crate::metrics::db::DBCollector::new(db_directory);
    registry.register(Box::new(db_collector))?;

    // Create an configure HTTP server
    let app = Router::new()
        .route("/metrics", get(collect_prometheus_metrics))
        .route("/stats/db", get(collect_db_metrics::<DB>))
        .with_state(db);

    // Wait for server to exit
    Ok(axum::serve(prometheus_listener, app.into_make_service()).await?)
}

async fn collect_prometheus_metrics() -> impl IntoResponse {
    let registry = prometheus::default_registry();
    let metric_families = registry.gather();
    let mut metrics = vec![];

    let encoder = TextEncoder::new();
    encoder
        .encode(&metric_families, &mut metrics)
        .expect("Encoding Prometheus metrics must succeed.");

    let mut text = String::new();
    match prometheus_client::encoding::text::encode(&mut text, &DEFAULT_REGISTRY.read()) {
        Ok(()) => metrics.extend_from_slice(text.as_bytes()),
        Err(e) => warn!("{e}"),
    };

    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        metrics,
    )
}

#[allow(clippy::unused_async)]
async fn collect_db_metrics<DB>(
    axum::extract::State(db): axum::extract::State<Arc<DB>>,
) -> impl IntoResponse
where
    DB: DBStatistics,
{
    let mut metrics = "# DB statistics:\n".to_owned();
    if let Some(db_stats) = db.get_statistics() {
        metrics.push_str(&db_stats);
    } else {
        metrics.push_str("Not enabled. Set enable_statistics to true in config and restart daemon");
    }
    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        metrics,
    )
}

pub mod labels {
    pub const KIND: &str = "kind";
}

pub mod values {
    /// `TipsetCache`.
    pub const TIPSET: &str = "tipset";
    /// tipset cache in state manager
    pub const STATE_MANAGER_TIPSET: &str = "sm_tipset";
}
