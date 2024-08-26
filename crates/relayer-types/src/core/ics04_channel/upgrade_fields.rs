use core::str::FromStr;

use ibc_proto::ibc::core::channel::v1::UpgradeFields as RawUpgradeFields;
use ibc_proto::Protobuf;
use itertools::Itertools;

use crate::core::ics04_channel::channel::Ordering;
use crate::core::ics04_channel::error::Error as ChannelError;
use crate::core::ics04_channel::version::Version;
use crate::core::ics24_host::identifier::ConnectionId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeFields {
    ordering: Ordering,
    connection_hops: Vec<ConnectionId>,
    version: Version,
}

impl UpgradeFields {
    pub fn new(ordering: Ordering, connection_hops: Vec<ConnectionId>, version: Version) -> Self {
        Self {
            ordering,
            connection_hops,
            version,
        }
    }
}

impl Protobuf<RawUpgradeFields> for UpgradeFields {}

impl TryFrom<RawUpgradeFields> for UpgradeFields {
    type Error = ChannelError;

    fn try_from(value: RawUpgradeFields) -> Result<Self, Self::Error> {
        use itertools::Either;

        let ordering = Ordering::from_i32(value.ordering)?;

        let (connection_hops, failures): (Vec<_>, Vec<_>) = value
            .connection_hops
            .iter()
            .partition_map(|id| match ConnectionId::from_str(id) {
                Ok(connection_id) => Either::Left(connection_id),
                Err(e) => Either::Right((id.clone(), e)),
            });

        if !failures.is_empty() {
            return Err(Self::Error::parse_connection_hops_vector(failures));
        }

        let version = Version::from(value.version);

        Ok(Self::new(ordering, connection_hops, version))
    }
}

impl From<UpgradeFields> for RawUpgradeFields {
    fn from(value: UpgradeFields) -> Self {
        let raw_connection_hops = value
            .connection_hops
            .iter()
            .map(|id| id.to_string())
            .collect();
        Self {
            ordering: value.ordering as i32,
            connection_hops: raw_connection_hops,
            version: value.version.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use std::string::ToString;
    use std::vec;

    use ibc_proto::ibc::core::channel::v1::UpgradeFields as RawUpgradeFields;

    use crate::core::ics04_channel::version::Version;

    pub fn get_dummy_upgrade_fields() -> RawUpgradeFields {
        RawUpgradeFields {
            ordering: 1,
            connection_hops: vec![],
            version: Version::ics20_with_fee(1).to_string(),
        }
    }
}
