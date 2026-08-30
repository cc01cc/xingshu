pub trait ProgressReporter: Send + Sync {
    fn started(&self, total: Option<u64>);
    fn item_finished(&self, item: &str, result: &str, duration_ms: Option<u64>);
    fn finished(&self, result: &str);
}
