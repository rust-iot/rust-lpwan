

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
    pub struct MockTimer (pub u64);

    impl super::Timer for MockTimer {
        fn ticks_ms(&self) -> u32 {
            return (self.0 / 1000) as u32
        }

        fn time_us(&self) -> u32 {
            return self.0 as u32
        }
    }
}
