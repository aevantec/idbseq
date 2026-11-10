//! Prometheus metrics (US-0034): served at `GET /metrics` when enabled.

use std::sync::Mutex;
use std::sync::atomic::AtomicU64;

use idbseq_core::collection::Collection;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::{Histogram, exponential_buckets};
use prometheus_client::registry::Registry;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct CollectionLabel {
    collection: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct StatusLabel {
    status: u16,
}

pub struct Metrics {
    registry: Mutex<Registry>,
    ids_issued: Family<CollectionLabel, Counter>,
    requests: Family<StatusLabel, Counter>,
    auth_failures: Counter,
    latency: Histogram,
    capacity: Family<CollectionLabel, Gauge<f64, AtomicU64>>,
}

impl std::fmt::Debug for Metrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Metrics").finish_non_exhaustive()
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    #[must_use]
    pub fn new() -> Self {
        let ids_issued = Family::<CollectionLabel, Counter>::default();
        let requests = Family::<StatusLabel, Counter>::default();
        let auth_failures = Counter::default();
        let latency = Histogram::new(exponential_buckets(0.000_1, 2.0, 16));
        let capacity = Family::<CollectionLabel, Gauge<f64, AtomicU64>>::default();
        let mut registry = Registry::with_prefix("idbseq");
        registry.register(
            "ids_issued",
            "IDs issued per collection",
            ids_issued.clone(),
        );
        registry.register("http_requests", "HTTP requests by status", requests.clone());
        registry.register(
            "auth_failures",
            "Requests rejected for missing or invalid credentials",
            auth_failures.clone(),
        );
        registry.register(
            "http_request_duration_seconds",
            "HTTP request latency",
            latency.clone(),
        );
        registry.register(
            "collection_remaining_capacity_ratio",
            "Share of a collection's ID range not yet issued (1 = unused, 0 = exhausted)",
            capacity.clone(),
        );
        Self {
            registry: Mutex::new(registry),
            ids_issued,
            requests,
            auth_failures,
            latency,
            capacity,
        }
    }

    pub fn ids_issued(&self, collection: &str, count: u32) {
        self.ids_issued
            .get_or_create(&CollectionLabel {
                collection: collection.to_owned(),
            })
            .inc_by(u64::from(count));
    }

    pub fn request(&self, status: u16, seconds: f64) {
        self.requests.get_or_create(&StatusLabel { status }).inc();
        self.latency.observe(seconds);
    }

    pub fn auth_failure(&self) {
        self.auth_failures.inc();
    }

    /// Render the text exposition, refreshing the capacity gauges from `collections`.
    #[must_use]
    pub fn render(&self, collections: &[Collection]) -> String {
        self.capacity.clear();
        for collection in collections {
            self.capacity
                .get_or_create(&CollectionLabel {
                    collection: collection.name().to_owned(),
                })
                .set(remaining_ratio(collection));
        }
        let mut out = String::new();
        let registry = self
            .registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if encode(&mut out, &registry).is_err() {
            return String::new();
        }
        out
    }
}

/// `(max - next + 1) / (max - start + 1)`, or 0 when exhausted.
#[allow(clippy::cast_precision_loss)] // a ratio needs no more than f64 precision
fn remaining_ratio(collection: &Collection) -> f64 {
    let spec = collection.spec();
    let total = spec.max.abs_diff(spec.start) as f64 + 1.0;
    collection
        .next_id()
        .map_or(0.0, |next| (spec.max.abs_diff(next) as f64 + 1.0) / total)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::time::SystemTime;

    use idbseq_core::collection::CollectionSpec;

    use super::*;

    fn collection(max: i64, issued: u32) -> Collection {
        let spec = CollectionSpec {
            max,
            ..CollectionSpec::named("orders")
        };
        let fresh = Collection::create(spec, SystemTime::UNIX_EPOCH).expect("valid");
        match NonZeroU32::new(issued) {
            Some(n) => fresh.advance(n).expect("advance").0,
            None => fresh,
        }
    }

    #[test]
    fn renders_counters_histogram_and_capacity() {
        let metrics = Metrics::new();
        metrics.ids_issued("orders", 5);
        metrics.request(200, 0.002);
        metrics.auth_failure();

        let text = metrics.render(&[collection(100, 25)]);

        assert!(
            text.contains(r#"idbseq_ids_issued_total{collection="orders"} 5"#),
            "{text}"
        );
        assert!(text.contains(r#"idbseq_http_requests_total{status="200"} 1"#));
        assert!(text.contains("idbseq_auth_failures_total 1"));
        assert!(text.contains("idbseq_http_request_duration_seconds_count 1"));
        assert!(
            text.contains(
                r#"idbseq_collection_remaining_capacity_ratio{collection="orders"} 0.75"#
            )
        );
    }

    #[test]
    fn capacity_is_one_when_unused_and_zero_when_exhausted() {
        assert!((remaining_ratio(&collection(10, 0)) - 1.0).abs() < f64::EPSILON);
        assert!(remaining_ratio(&collection(10, 10)).abs() < f64::EPSILON);
    }
}
