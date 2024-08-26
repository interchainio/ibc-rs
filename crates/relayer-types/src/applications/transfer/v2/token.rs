use serde::Deserialize;
use serde::Serialize;

use ibc_proto::ibc::applications::transfer::v2::Token as RawToken;

use crate::applications::transfer::error::Error;
use crate::applications::transfer::v2::denom::Denom;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub denom: Option<Denom>,
    pub amount: String,
}

impl From<Token> for RawToken {
    fn from(value: Token) -> Self {
        RawToken {
            denom: value.denom.map(|d| d.into()),
            amount: value.amount,
        }
    }
}

impl TryFrom<RawToken> for Token {
    type Error = Error;

    fn try_from(value: RawToken) -> Result<Self, Self::Error> {
        let raw_denom = value.denom.map(Denom::try_from).transpose()?;
        Ok(Token {
            denom: raw_denom,
            amount: value.amount,
        })
    }
}
