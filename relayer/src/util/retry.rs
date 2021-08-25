use core::time::Duration;

pub use retry::{
    delay::{Fibonacci, Fixed},
    retry_with_index, Error as RetryError, OperationResult as RetryResult,
};

pub fn retry_count<E>(err: &RetryError<E>) -> u64 {
    match err {
        RetryError::Operation {
            tries,
            total_delay: _,
            error: _,
        } => *tries,
        _ => 0,
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ConstantGrowth {
    delay: Duration,
    incr: Duration,
}

impl ConstantGrowth {
    pub const fn new(delay: Duration, incr: Duration) -> Self {
        Self { delay, incr }
    }

    pub fn clamp(self, max_delay: Duration, max_retries: usize) -> impl Iterator<Item = Duration> {
        clamp(self, max_delay, max_retries)
    }
}

impl From<Duration> for ConstantGrowth {
    fn from(delay: Duration) -> Self {
        Self::new(delay, Duration::from_secs(1))
    }
}

impl Iterator for ConstantGrowth {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        let delay = self.delay;

        if let Some(next) = self.delay.checked_add(self.incr) {
            self.delay = next;
        }

        Some(delay)
    }
}

pub fn clamp(
    strategy: impl Iterator<Item = Duration>,
    max_delay: Duration,
    max_retries: usize,
) -> impl Iterator<Item = Duration> {
    strategy
        .take(max_retries)
        .map(move |delay| delay.min(max_delay))
}

pub fn clamp_total(
    strategy: impl Iterator<Item = Duration>,
    max_delay: Duration,
    max_total_delay: Duration,
) -> impl Iterator<Item = Duration> {
    strategy.map(move |delay| delay.min(max_delay)).scan(
        Duration::from_millis(0),
        move |elapsed, delay| {
            let next = if *elapsed >= max_total_delay {
                None
            } else if (*elapsed + delay) > max_total_delay {
                Some(max_total_delay - *elapsed)
            } else {
                Some(delay)
            };

            *elapsed += delay;
            next
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_env_log::test;

    const CONST_STRATEGY: ConstantGrowth =
        ConstantGrowth::new(Duration::from_secs(1), Duration::from_millis(500));

    #[test]
    fn const_growth_no_clamp() {
        let delays = CONST_STRATEGY.take(10).collect::<Vec<_>>();
        assert_eq!(
            delays,
            vec![
                Duration::from_millis(1000),
                Duration::from_millis(1500),
                Duration::from_millis(2000),
                Duration::from_millis(2500),
                Duration::from_millis(3000),
                Duration::from_millis(3500),
                Duration::from_millis(4000),
                Duration::from_millis(4500),
                Duration::from_millis(5000),
                Duration::from_millis(5500)
            ]
        );
    }

    #[test]
    fn clamped_const_growth_max_delay() {
        let strategy = CONST_STRATEGY.clamp(Duration::from_secs(10), 10);
        let delays = strategy.collect::<Vec<_>>();
        assert_eq!(
            delays,
            vec![
                Duration::from_millis(1000),
                Duration::from_millis(1500),
                Duration::from_millis(2000),
                Duration::from_millis(2500),
                Duration::from_millis(3000),
                Duration::from_millis(3500),
                Duration::from_millis(4000),
                Duration::from_millis(4500),
                Duration::from_millis(5000),
                Duration::from_millis(5500)
            ]
        );
    }

    #[test]
    fn clamped_const_growth_max_retries() {
        let strategy = CONST_STRATEGY.clamp(Duration::from_secs(10000), 5);
        let delays = strategy.collect::<Vec<_>>();
        assert_eq!(
            delays,
            vec![
                Duration::from_millis(1000),
                Duration::from_millis(1500),
                Duration::from_millis(2000),
                Duration::from_millis(2500),
                Duration::from_millis(3000)
            ]
        );
    }

    #[test]
    fn clamped_total_const_growth_max_retries() {
        const MAX_DELAY: Duration = Duration::from_millis(500);
        const DELAY_INCR: Duration = Duration::from_millis(100);
        const INITIAL_DELAY: Duration = Duration::from_millis(200);
        const MAX_RETRY_DURATION: Duration = Duration::from_secs(2);

        let strategy = clamp_total(
            ConstantGrowth::new(INITIAL_DELAY, DELAY_INCR),
            MAX_DELAY,
            MAX_RETRY_DURATION,
        );

        let delays = strategy.collect::<Vec<_>>();

        assert_eq!(
            delays,
            vec![
                Duration::from_millis(200),
                Duration::from_millis(300),
                Duration::from_millis(400),
                Duration::from_millis(500),
                Duration::from_millis(500),
                Duration::from_millis(100)
            ]
        );
    }
}
