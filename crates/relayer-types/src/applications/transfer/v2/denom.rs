use serde::Deserialize;
use serde::Serialize;

use ibc_proto::ibc::applications::transfer::v2::Denom as RawDenom;

use crate::applications::transfer::error::Error;
use crate::applications::transfer::v1::hop::Hop;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Denom {
    pub base: String,
    pub trace: Vec<Hop>,
}

impl From<Denom> for RawDenom {
    fn from(value: Denom) -> Self {
        let raw_trace = value.trace.into_iter().map(|trace| trace.into()).collect();
        RawDenom {
            base: value.base,
            trace: raw_trace,
        }
    }
}

impl TryFrom<RawDenom> for Denom {
    type Error = Error;

    fn try_from(value: RawDenom) -> Result<Self, Self::Error> {
        let raw_trace = value
            .trace
            .into_iter()
            .map(Hop::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Denom {
            base: value.base,
            trace: raw_trace,
        })
    }
}
