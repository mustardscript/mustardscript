use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    state: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.state.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::SeqCst)
    }

    #[doc(hidden)]
    pub fn from_shared(state: Arc<AtomicBool>) -> Self {
        Self { state }
    }
}
