use crate::base::one_for_all::traits::error::{OfaError, OfaErrorContext};
use crate::base::relay::impls::packet_relayers::retry::{MaxRetryExceeded, RetryableError};
use crate::base::relay::traits::ibc_message_sender::MismatchIbcEventsCountError;

impl<Error: OfaError> RetryableError for OfaErrorContext<Error> {
    fn is_retryable(&self) -> bool {
        self.error.is_retryable()
    }
}

impl<Error> From<Error> for OfaErrorContext<Error> {
    fn from(error: Error) -> Self {
        Self { error }
    }
}

impl<Error: OfaError> From<MismatchIbcEventsCountError> for OfaErrorContext<Error> {
    fn from(e: MismatchIbcEventsCountError) -> Self {
        Self::new(Error::mismatch_ibc_events_count(e.expected, e.actual))
    }
}

impl<Error: OfaError> From<MaxRetryExceeded> for OfaErrorContext<Error> {
    fn from(e: MaxRetryExceeded) -> Self {
        Self::new(Error::max_retry_exceeded(e.retries))
    }
}
