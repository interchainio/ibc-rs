use crate::prelude::*;
use core::convert::TryInto;
use core::fmt::Display;
use core::num::{ParseIntError, TryFromIntError};
use core::ops::{Add, Sub};
use core::str::FromStr;
use core::time::Duration;

use chrono::{offset::Utc, DateTime, TimeZone};
use flex_error::{define_error, TraceError};
use serde_derive::{Deserialize, Serialize};

pub const ZERO_DURATION: Duration = Duration::from_secs(0);

/// A newtype wrapper over `Option<DateTime<Utc>>` to keep track of
/// IBC packet timeout.
///
/// We use an explicit `Option` type to distinguish this when converting between
/// a `u64` value and a raw timestamp. In protocol buffer, the timestamp is
/// represented as a `u64` Unix timestamp in nanoseconds, with 0 representing the absence
/// of timestamp.
#[derive(PartialEq, Eq, Copy, Clone, Debug, Deserialize, Serialize, Hash)]
pub struct Timestamp {
    time: Option<DateTime<Utc>>,
}

/// The expiry result when comparing two timestamps.
/// - If either timestamp is invalid (0), the result is `InvalidTimestamp`.
/// - If the left timestamp is strictly after the right timestamp, the result is `Expired`.
/// - Otherwise, the result is `NotExpired`.
///
/// User of this result may want to determine whether error should be raised,
/// when either of the timestamp being compared is invalid.
#[derive(PartialEq, Eq, Copy, Clone, Debug, Deserialize, Serialize, Hash)]
pub enum Expiry {
    Expired,
    NotExpired,
    InvalidTimestamp,
}

impl Timestamp {
    /// When used in IBC, all raw timestamps are represented as u64 Unix timestamp in nanoseconds.
    ///
    /// A value of 0 indicates that the timestamp is not set, and result in the underlying
    /// type being None.
    ///
    /// The underlying library [`chrono::DateTime`] allows conversion from nanoseconds only
    /// from an `i64` value. In practice, `i64` still have sufficient precision for our purpose.
    /// However we have to handle the case of `u64` overflowing in `i64`, to prevent
    /// malicious packets from crashing the relayer.
    pub fn from_nanoseconds(nanoseconds: u64) -> Result<Timestamp, TryFromIntError> {
        if nanoseconds == 0 {
            Ok(Timestamp { time: None })
        } else {
            let nanoseconds = nanoseconds.try_into()?;
            Ok(Timestamp {
                time: Some(Utc.timestamp_nanos(nanoseconds)),
            })
        }
    }

    /// Returns a `Timestamp` representation of the current time.
    pub fn now() -> Timestamp {
        Timestamp {
            time: Some(Utc::now()),
        }
    }

    /// Returns a `Timestamp` representation of a timestamp not being set.
    pub fn none() -> Self {
        Timestamp { time: None }
    }

    /// Computes the duration difference of another `Timestamp` from the current one.
    /// Returns the difference in time as an [`core::time::Duration`].
    /// Returns `None` if the other `Timestamp` is more advanced
    /// than the current or if either of the `Timestamp`s is not set.
    pub fn duration_since(&self, other: &Timestamp) -> Option<Duration> {
        match (self.time, other.time) {
            (Some(time1), Some(time2)) => time1.signed_duration_since(time2).to_std().ok(),
            _ => None,
        }
    }

    /// Convert a `Timestamp` from [`chrono::DateTime<Utc>`].
    pub fn from_datetime(time: DateTime<Utc>) -> Timestamp {
        Timestamp { time: Some(time) }
    }

    /// Convert a `Timestamp` to `u64` value in nanoseconds. If no timestamp
    /// is set, the result is 0.
    pub fn as_nanoseconds(&self) -> u64 {
        self.time
            .map_or(0, |time| time.timestamp_nanos().try_into().unwrap())
    }

    /// Convert a `Timestamp` to an optional [`chrono::DateTime<Utc>`]
    pub fn as_datetime(&self) -> Option<DateTime<Utc>> {
        self.time
    }

    /// Checks whether the timestamp has expired when compared to the
    /// `other` timestamp. Returns an [`Expiry`] result.
    pub fn check_expiry(&self, other: &Timestamp) -> Expiry {
        match (self.time, other.time) {
            (Some(time1), Some(time2)) => {
                if time1 > time2 {
                    Expiry::Expired
                } else {
                    Expiry::NotExpired
                }
            }
            _ => Expiry::InvalidTimestamp,
        }
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Timestamp({})",
            self.time
                .map_or("NoTimestamp".to_string(), |time| time.to_rfc3339())
        )
    }
}

define_error! {
    TimestampOverflowError {
        TimestampOverflow
            |_| { "Timestamp overflow when modifying with duration" }
    }
}

impl Add<Duration> for Timestamp {
    type Output = Result<Timestamp, TimestampOverflowError>;

    fn add(self, duration: Duration) -> Result<Timestamp, TimestampOverflowError> {
        match self.as_datetime() {
            Some(datetime) => {
                let duration2 = chrono::Duration::from_std(duration)
                    .map_err(|_| TimestampOverflowError::timestamp_overflow())?;
                Ok(Self::from_datetime(datetime + duration2))
            }
            None => Ok(self),
        }
    }
}

impl Sub<Duration> for Timestamp {
    type Output = Result<Timestamp, TimestampOverflowError>;

    fn sub(self, duration: Duration) -> Result<Timestamp, TimestampOverflowError> {
        match self.as_datetime() {
            Some(datetime) => {
                let duration2 = chrono::Duration::from_std(duration)
                    .map_err(|_| TimestampOverflowError::timestamp_overflow())?;
                Ok(Self::from_datetime(datetime - duration2))
            }
            None => Ok(self),
        }
    }
}

define_error! {
    ParseTimestampError {
        ParseInt
            [ TraceError<ParseIntError> ]
            | _ | { "error parsing integer from string"},

        TryFromInt
            [ TraceError<TryFromIntError> ]
            | _ | { "error converting from u64 to i64" },
    }
}

impl FromStr for Timestamp {
    type Err = ParseTimestampError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let seconds = u64::from_str(s).map_err(ParseTimestampError::parse_int)?;

        Timestamp::from_nanoseconds(seconds).map_err(ParseTimestampError::try_from_int)
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Timestamp { time: None }
    }
}

#[cfg(test)]
mod tests {
    use core::convert::TryInto;
    use core::time::Duration;
    use std::thread::sleep;
    use test_env_log::test;

    use super::{Expiry, Timestamp, ZERO_DURATION};

    #[test]
    fn test_timestamp_comparisons() {
        let nil_timestamp = Timestamp::from_nanoseconds(0).unwrap();
        assert_eq!(nil_timestamp.time, None);
        assert_eq!(nil_timestamp.as_nanoseconds(), 0);

        let timestamp1 = Timestamp::from_nanoseconds(1).unwrap();
        assert_eq!(timestamp1.time.unwrap().timestamp(), 0);
        assert_eq!(timestamp1.time.unwrap().timestamp_millis(), 0);
        assert_eq!(timestamp1.time.unwrap().timestamp_nanos(), 1);
        assert_eq!(timestamp1.as_nanoseconds(), 1);

        let timestamp2 = Timestamp::from_nanoseconds(1_000_000_000).unwrap();
        assert_eq!(timestamp2.time.unwrap().timestamp(), 1);
        assert_eq!(timestamp2.time.unwrap().timestamp_millis(), 1_000);
        assert_eq!(timestamp2.as_nanoseconds(), 1_000_000_000);

        assert!(Timestamp::from_nanoseconds(u64::MAX).is_err());
        assert!(Timestamp::from_nanoseconds(i64::MAX.try_into().unwrap()).is_ok());

        assert_eq!(timestamp1.check_expiry(&timestamp2), Expiry::NotExpired);
        assert_eq!(timestamp1.check_expiry(&timestamp1), Expiry::NotExpired);
        assert_eq!(timestamp2.check_expiry(&timestamp2), Expiry::NotExpired);
        assert_eq!(timestamp2.check_expiry(&timestamp1), Expiry::Expired);
        assert_eq!(
            timestamp1.check_expiry(&nil_timestamp),
            Expiry::InvalidTimestamp
        );
        assert_eq!(
            nil_timestamp.check_expiry(&timestamp2),
            Expiry::InvalidTimestamp
        );
        assert_eq!(
            nil_timestamp.check_expiry(&nil_timestamp),
            Expiry::InvalidTimestamp
        );
    }

    #[test]
    fn test_timestamp_arithmetic() {
        let time0 = Timestamp::none();
        let time1 = Timestamp::from_nanoseconds(100).unwrap();
        let time2 = Timestamp::from_nanoseconds(150).unwrap();
        let time3 = Timestamp::from_nanoseconds(50).unwrap();
        let duration = Duration::from_nanos(50);

        assert_eq!(time1, (time1 + ZERO_DURATION).unwrap());
        assert_eq!(time2, (time1 + duration).unwrap());
        assert_eq!(time3, (time1 - duration).unwrap());
        assert_eq!(time0, (time0 + duration).unwrap());
        assert_eq!(time0, (time0 - duration).unwrap());
    }

    #[test]
    fn subtract_compare() {
        let sleep_duration = Duration::from_micros(100);

        let start = Timestamp::now();
        sleep(sleep_duration);
        let end = Timestamp::now();

        let res = end.duration_since(&start);
        assert!(res.is_some());

        let inner = res.unwrap();
        assert!(inner > sleep_duration);
    }
}
