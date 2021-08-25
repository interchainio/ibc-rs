use crate::util::retry::{clamp_total, ConstantGrowth};
use core::time::Duration;

const MAX_DELAY: Duration = Duration::from_millis(500);
const DELAY_INCR: Duration = Duration::from_millis(100);
const INITIAL_DELAY: Duration = Duration::from_millis(200);
const MAX_RETRY_DURATION: Duration = Duration::from_secs(2);

pub fn worker_default_strategy() -> impl Iterator<Item = Duration> {
    let strategy = ConstantGrowth::new(INITIAL_DELAY, DELAY_INCR);
    clamp_total(strategy, MAX_DELAY, MAX_RETRY_DURATION)
}
