//! Prometheus-style metrics for serve observability.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct TenantCounters {
    experiences: u64,
    activates: u64,
}

#[derive(Debug, Default)]
pub struct Metrics {
    pub experiences: AtomicU64,
    pub activates: AtomicU64,
    pub sleeps: AtomicU64,
    pub compactions: AtomicU64,
    pub wal_replays: AtomicU64,
    pub activate_ms_total: AtomicU64,
    pub experience_ms_total: AtomicU64,
    pub checkpoint_ms_total: AtomicU64,
    pub activate_count: AtomicU64,
    pub experience_count: AtomicU64,
    pub checkpoint_count: AtomicU64,
    pub synapse_count: AtomicU64,
    pub active_connections: AtomicU64,
    pub rejected_connections: AtomicU64,
    tenant: Mutex<std::collections::HashMap<String, TenantCounters>>,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn record_activate(&self, ms: u64) {
        self.activates.fetch_add(1, Ordering::Relaxed);
        self.activate_ms_total.fetch_add(ms, Ordering::Relaxed);
        self.activate_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_experience(&self, ms: u64) {
        self.experiences.fetch_add(1, Ordering::Relaxed);
        self.experience_ms_total.fetch_add(ms, Ordering::Relaxed);
        self.experience_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_tenant_experience(&self, tenant_id: &str) {
        if let Ok(mut map) = self.tenant.lock() {
            map.entry(tenant_id.to_string()).or_default().experiences += 1;
        }
    }

    pub fn record_tenant_activate(&self, tenant_id: &str) {
        if let Ok(mut map) = self.tenant.lock() {
            map.entry(tenant_id.to_string()).or_default().activates += 1;
        }
    }

    pub fn record_checkpoint(&self, ms: u64) {
        self.checkpoint_ms_total.fetch_add(ms, Ordering::Relaxed);
        self.checkpoint_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_synapses(&self, n: usize) {
        self.synapse_count.store(n as u64, Ordering::Relaxed);
    }

    pub fn render_prometheus(&self) -> String {
        let avg = |total: u64, count: u64| {
            if count == 0 {
                0.0
            } else {
                total as f64 / count as f64
            }
        };
        format!(
            concat!(
                "# HELP fluctlight_experiences_total Experiences encoded\n",
                "# TYPE fluctlight_experiences_total counter\n",
                "fluctlight_experiences_total {}\n",
                "# HELP fluctlight_activates_total Activation requests\n",
                "# TYPE fluctlight_activates_total counter\n",
                "fluctlight_activates_total {}\n",
                "# HELP fluctlight_sleeps_total Sleep cycles\n",
                "# TYPE fluctlight_sleeps_total counter\n",
                "fluctlight_sleeps_total {}\n",
                "# HELP fluctlight_compactions_total Compaction runs\n",
                "# TYPE fluctlight_compactions_total counter\n",
                "fluctlight_compactions_total {}\n",
                "# HELP fluctlight_wal_replays_total WAL replay operations\n",
                "# TYPE fluctlight_wal_replays_total counter\n",
                "fluctlight_wal_replays_total {}\n",
                "# HELP fluctlight_synapse_count Current synapse count\n",
                "# TYPE fluctlight_synapse_count gauge\n",
                "fluctlight_synapse_count {}\n",
                "# HELP fluctlight_activate_ms_avg Average activate latency ms\n",
                "# TYPE fluctlight_activate_ms_avg gauge\n",
                "fluctlight_activate_ms_avg {:.3}\n",
                "# HELP fluctlight_experience_ms_avg Average experience latency ms\n",
                "# TYPE fluctlight_experience_ms_avg gauge\n",
                "fluctlight_experience_ms_avg {:.3}\n",
                "# HELP fluctlight_checkpoint_ms_avg Average checkpoint latency ms\n",
                "# TYPE fluctlight_checkpoint_ms_avg gauge\n",
                "fluctlight_checkpoint_ms_avg {:.3}\n",
                "# HELP fluctlight_active_connections Active HTTP connections\n",
                "# TYPE fluctlight_active_connections gauge\n",
                "fluctlight_active_connections {}\n",
                "# HELP fluctlight_rejected_connections_total Rejected due to limit\n",
                "# TYPE fluctlight_rejected_connections_total counter\n",
                "fluctlight_rejected_connections_total {}\n",
            ),
            self.experiences.load(Ordering::Relaxed),
            self.activates.load(Ordering::Relaxed),
            self.sleeps.load(Ordering::Relaxed),
            self.compactions.load(Ordering::Relaxed),
            self.wal_replays.load(Ordering::Relaxed),
            self.synapse_count.load(Ordering::Relaxed),
            avg(
                self.activate_ms_total.load(Ordering::Relaxed),
                self.activate_count.load(Ordering::Relaxed),
            ),
            avg(
                self.experience_ms_total.load(Ordering::Relaxed),
                self.experience_count.load(Ordering::Relaxed),
            ),
            avg(
                self.checkpoint_ms_total.load(Ordering::Relaxed),
                self.checkpoint_count.load(Ordering::Relaxed),
            ),
            self.active_connections.load(Ordering::Relaxed),
            self.rejected_connections.load(Ordering::Relaxed),
        )
    }

    pub fn tenant_snapshot(&self) -> std::collections::HashMap<String, TenantCounters> {
        self.tenant.lock().map(|m| m.clone()).unwrap_or_default()
    }
}

pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
}
