#[cfg(feature = "profiling")]
pub mod profiling {

    use core::sync::atomic::AtomicUsize;
    use core::sync::atomic::Ordering::Relaxed;

    std::thread_local! {
        pub static DEPTH: AtomicUsize = AtomicUsize::new(0);
    }

    /// Measure the time between when this value is allocated
    /// and when it is dropped.
    pub struct Timer {
        name: String,
        start: std::time::Instant,
    }

    impl Timer {
        pub fn new(name: String) -> Self {
            let depth = DEPTH.with(|d| d.fetch_add(1, Relaxed));
            let pad = "   ".repeat(depth);

            tracing::info!("{}⏳ {} - start", pad, name);

            Self {
                name,
                start: std::time::Instant::now(),
            }
        }
    }

    impl Drop for Timer {
        fn drop(&mut self) {
            let elapsed = self.start.elapsed().as_millis();

            let depth = DEPTH.with(|d| d.fetch_sub(1, Relaxed));
            let pad = "   ".repeat(depth - 1);

            tracing::info!("{}⏳ {} - elapsed: {}ms", pad, self.name, elapsed);
        }
    }
}

/// Measure the time until the current scope ends.
///
/// Only enabled when the "profiling" feature is enabled.
///
/// ## Example
///
/// ```rust
/// use ibc_relayer::time;
///
/// time!("full scope");
///
/// let x = {
///   time!("inner {}", "scope");
///
///   42
///   // "inner scope" timer ends here
/// };
/// // "full scope" timer ends here
/// ```
#[macro_export]
macro_rules! time {
    ($($arg:tt)*) => {
        #[cfg(feature = "profiling")]
        let _timer = $crate::macros::profiling::Timer::new(format!($($arg)*));
    };
}
