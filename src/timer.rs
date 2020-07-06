

/// Timer trait provides mechanisms for accessing monotonic times
/// to assist with procotol implementations.
///
/// All methods are monotonic and relative to the same unknown epoc
pub trait Timer {
    /// Returns the number of millisecond ticks since some unknown epoc
    fn ticks_ms(&self) -> u32;

    /// Returns the microsecond ticks since some unknown epoc
    fn time_us(&self) -> u32;
}

#[cfg(any(test, feature="mocks"))]
pub mod mock {
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug)]
    pub struct MockTimer (Arc<Mutex<u64>>);

    impl MockTimer {
        pub fn new() -> Self {
            Self(Arc::new(Mutex::new(0)))
        }

        pub fn set_ms(&mut self, val: u32) {
            *self.0.lock().unwrap() = val as u64 * 1000;
        }

        pub fn inc(&mut self) {
            let mut v  = self.0.lock().unwrap();
            *v += 1000;
        }

        pub fn val(&self) -> u32 {
            (*self.0.lock().unwrap() / 1000) as u32
        }
    }

    impl super::Timer for MockTimer {
        fn ticks_ms(&self) -> u32 {
            let v = self.0.lock().unwrap();
            return (*v / 1000) as u32
        }

        fn time_us(&self) -> u32 {
            let v = self.0.lock().unwrap();
            return *v as u32
        }
    }
}
