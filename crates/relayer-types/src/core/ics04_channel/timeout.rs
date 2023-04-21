use crate::prelude::*;

use core::fmt::{Display, Error as FmtError, Formatter};

use serde::{Deserialize, Serialize};

use ibc_proto::ibc::core::channel::v1::UpgradeTimeout as RawUpgradeTimeout;
use ibc_proto::ibc::core::client::v1::Height as RawHeight;
use ibc_proto::protobuf::Protobuf;

use crate::core::ics02_client::{error::Error as ICS2Error, height::Height};
use crate::core::ics04_channel::error::Error as ChannelError;
use crate::timestamp::Timestamp;

/// Indicates a consensus height on the destination chain after which the packet
/// will no longer be processed, and will instead count as having timed-out.
///
/// `TimeoutHeight` is treated differently from other heights because
///
/// `RawHeight.timeout_height == {revision_number: 0, revision_height = 0}`
///
/// is legal and meaningful, even though the Tendermint spec rejects this height
/// as invalid. Thus, it must be parsed specially, where this special case means
/// "no timeout".
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum TimeoutHeight {
    Never,
    At(Height),
}

impl TimeoutHeight {
    pub fn no_timeout() -> Self {
        Self::Never
    }

    /// Revision number to be used in packet commitment computation
    pub fn commitment_revision_number(&self) -> u64 {
        match self {
            Self::At(height) => height.revision_number(),
            Self::Never => 0,
        }
    }

    /// Revision height to be used in packet commitment computation
    pub fn commitment_revision_height(&self) -> u64 {
        match self {
            Self::At(height) => height.revision_height(),
            Self::Never => 0,
        }
    }

    /// Check if a height is *stricly past* the timeout height, and thus is
    /// deemed expired.
    pub fn has_expired(&self, height: Height) -> bool {
        match self {
            Self::At(timeout_height) => height > *timeout_height,
            // When there's no timeout, heights are never expired
            Self::Never => false,
        }
    }

    /// Returns the height formatted for an ABCI event attribute value.
    pub fn to_event_attribute_value(self) -> String {
        match self {
            TimeoutHeight::At(height) => height.to_string(),
            TimeoutHeight::Never => "0-0".into(),
        }
    }
}

impl Default for TimeoutHeight {
    fn default() -> Self {
        Self::Never
    }
}

impl TryFrom<RawHeight> for TimeoutHeight {
    type Error = ICS2Error;

    // Note: it is important for `revision_number` to also be `0`, otherwise
    // packet commitment proofs will be incorrect (proof construction in
    // `ChannelReader::packet_commitment()` uses both `revision_number` and
    // `revision_height`). Note also that ibc-go conforms to this convention.
    fn try_from(raw_height: RawHeight) -> Result<Self, Self::Error> {
        if raw_height.revision_number == 0 && raw_height.revision_height == 0 {
            Ok(TimeoutHeight::Never)
        } else {
            let height: Height = raw_height.try_into()?;
            Ok(TimeoutHeight::At(height))
        }
    }
}

impl TryFrom<Option<RawHeight>> for TimeoutHeight {
    type Error = ICS2Error;

    fn try_from(maybe_raw_height: Option<RawHeight>) -> Result<Self, Self::Error> {
        match maybe_raw_height {
            Some(raw_height) => Self::try_from(raw_height),
            None => Ok(TimeoutHeight::Never),
        }
    }
}
/// We map "no timeout height" to `Some(RawHeight::zero)` due to a quirk
/// in ICS-4. See <https://github.com/cosmos/ibc/issues/776>.
impl From<TimeoutHeight> for Option<RawHeight> {
    fn from(timeout_height: TimeoutHeight) -> Self {
        let raw_height = match timeout_height {
            TimeoutHeight::At(height) => height.into(),
            TimeoutHeight::Never => RawHeight {
                revision_number: 0,
                revision_height: 0,
            },
        };

        Some(raw_height)
    }
}

impl From<Height> for TimeoutHeight {
    fn from(height: Height) -> Self {
        Self::At(height)
    }
}

impl Display for TimeoutHeight {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        match self {
            TimeoutHeight::At(timeout_height) => write!(f, "{timeout_height}"),
            TimeoutHeight::Never => write!(f, "no timeout"),
        }
    }
}

impl Serialize for TimeoutHeight {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // When there is no timeout, we cannot construct an ICS02 Height with
        // revision number and height at zero, so we have to define an
        // isomorphic struct to serialize it as if it were an ICS02 height.
        #[derive(Serialize)]
        struct Height {
            revision_number: u64,
            revision_height: u64,
        }

        match self {
            // If there is no timeout, we use our ad-hoc struct above
            TimeoutHeight::Never => {
                let zero = Height {
                    revision_number: 0,
                    revision_height: 0,
                };

                zero.serialize(serializer)
            }
            // Otherwise we can directly serialize the underlying height
            TimeoutHeight::At(height) => height.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for TimeoutHeight {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use crate::core::ics02_client::height::Height as Ics02Height;

        // Here we have to use a bespoke struct as well in order to deserialize
        // a height which may have a revision height equal to zero.
        #[derive(Deserialize)]
        struct Height {
            revision_number: u64,
            revision_height: u64,
        }

        Height::deserialize(deserializer).map(|height| {
            Ics02Height::new(height.revision_number, height.revision_height)
                // If it's a valid height with a non-zero revision height, then we have a timeout
                .map(TimeoutHeight::At)
                // Otherwise, no timeout
                .unwrap_or(TimeoutHeight::Never)
        })
    }
}

/// A composite of timeout height and timeout timestamp types, useful for when
/// performing a channel upgrade handshake, as there are cases when only timeout
/// height is set, only timeout timestamp is set, or both are set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpgradeTimeout {
    /// Timeout height indicates the height at which the counterparty
    /// must no longer proceed with the upgrade handshake.
    /// The chains will then preserve their original channel and the upgrade handshake is aborted
    Height(Height),

    /// Timeout timestamp indicates the time on the counterparty at which
    /// the counterparty must no longer proceed with the upgrade handshake.
    /// The chains will then preserve their original channel and the upgrade handshake is aborted.
    Timestamp(Timestamp),

    /// Both timeouts are set.
    Both(Height, Timestamp),
}

impl UpgradeTimeout {
    pub fn new(height: Option<Height>, timestamp: Option<Timestamp>) -> Result<Self, ChannelError> {
        match (height, timestamp) {
            (Some(height), None) => Ok(UpgradeTimeout::Height(height)),
            (None, Some(timestamp)) => Ok(UpgradeTimeout::Timestamp(timestamp)),
            (Some(height), Some(timestamp)) => Ok(UpgradeTimeout::Both(height, timestamp)),
            (None, None) => Err(ChannelError::missing_upgrade_timeout()),
        }
    }

    pub fn into_tuple(self) -> (Option<Height>, Option<Timestamp>) {
        match self {
            UpgradeTimeout::Height(height) => (Some(height), None),
            UpgradeTimeout::Timestamp(timestamp) => (None, Some(timestamp)),
            UpgradeTimeout::Both(height, timestamp) => (Some(height), Some(timestamp)),
        }
    }
}

impl Protobuf<RawUpgradeTimeout> for UpgradeTimeout {}

impl TryFrom<RawUpgradeTimeout> for UpgradeTimeout {
    type Error = ChannelError;

    fn try_from(value: RawUpgradeTimeout) -> Result<Self, Self::Error> {
        let timeout_height = value
            .height
            .map(Height::try_from)
            .transpose()
            .map_err(|_| Self::Error::invalid_timeout_height())?;

        let timeout_timestamp = Timestamp::from_nanoseconds(value.timestamp)
            .map_err(|_| Self::Error::invalid_timeout_timestamp)
            .ok()
            .filter(|ts| ts.nanoseconds() > 0);

        Self::new(timeout_height, timeout_timestamp)
    }
}

impl From<UpgradeTimeout> for RawUpgradeTimeout {
    fn from(value: UpgradeTimeout) -> Self {
        match value {
            UpgradeTimeout::Height(height) => Self {
                height: RawHeight::try_from(height).ok(),
                timestamp: 0,
            },
            UpgradeTimeout::Timestamp(timestamp) => Self {
                height: None,
                timestamp: timestamp.nanoseconds(),
            },
            UpgradeTimeout::Both(height, timestamp) => Self {
                height: RawHeight::try_from(height).ok(),
                timestamp: timestamp.nanoseconds(),
            },
        }
    }
}
