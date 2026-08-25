use super::*;

#[derive(Clone)]
pub(crate) struct DownloadCancellation {
    pub(crate) item_flag: Arc<AtomicBool>,
    pub(crate) generation: Arc<AtomicU64>,
    pub(crate) batch_generation: u64,
}

impl DownloadCancellation {
    pub(crate) fn is_cancelled(&self) -> bool {
        self.item_flag.load(Ordering::Acquire)
            || self.generation.load(Ordering::Acquire) != self.batch_generation
    }
}

pub(crate) async fn wait_for_cancellation(cancellation: &DownloadCancellation) {
    while !cancellation.is_cancelled() {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_generation_cancels_tasks_even_if_item_map_changes() {
        let generation = Arc::new(AtomicU64::new(7));
        let cancellation = DownloadCancellation {
            item_flag: Arc::new(AtomicBool::new(false)),
            generation: generation.clone(),
            batch_generation: 7,
        };
        assert!(!cancellation.is_cancelled());
        generation.fetch_add(1, Ordering::AcqRel);
        assert!(cancellation.is_cancelled());
    }
}
