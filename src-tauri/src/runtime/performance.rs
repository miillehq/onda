//! Owns content-free performance samples and resource reports.
use super::*;

#[derive(Default)]
pub(super) struct Performance {
    pub(super) metrics: Mutex<VecDeque<SpanMetric>>,
}

impl Performance {
    pub(super) fn metric(
        &self,
        host: &Runtime,
        id: &str,
        subsystem: &str,
        started: Instant,
        success: bool,
    ) {
        let metric = SpanMetric {
            operation_id: id.into(),
            subsystem: subsystem.into(),
            duration_ms: started.elapsed().as_secs_f64() * 1000.0,
            success,
        };
        if let Ok(mut metrics) = lock(&self.metrics) {
            if metrics.len() >= MAX_METRICS {
                metrics.pop_front();
            }
            metrics.push_back(metric.clone());
        }
        host.domain(AppEvent::Metric(metric));
    }
}
