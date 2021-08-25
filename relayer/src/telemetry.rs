// If the `telemetry` feature is enabled, re-export the `ibc-telemetry` state.
#[cfg(feature = "telemetry")]
pub type Telemetry = alloc::sync::Arc<ibc_telemetry::TelemetryState>;

// Otherwise, define and export a dummy type.
#[cfg(not(feature = "telemetry"))]
#[derive(Clone, Debug)]
pub struct TelemetryDisabled;

#[cfg(not(feature = "telemetry"))]
pub type Telemetry = TelemetryDisabled;

/// A macro to send metric updates via a telemetry handle,
/// only if the `telemetry` feature is enabled.
/// Otherwise, it compiles to a no-op.
///
/// ## Note
/// Equivalent to `#[cfg(feature = "telemetry")]`, but
/// should be preferred over the latter.
///
/// ## Example
///
/// ```rust,ignore
/// telemetry!(self.telemetry.tx_count(1));
/// ```
#[macro_export]
macro_rules! telemetry {
    ($e:expr) => {
        #[cfg(feature = "telemetry")]
        {
            $e;
        }
    };
}
