use core::fmt::{self, Display};
use core::ops::{Add, Sub};
use eyre::eyre;
use ibc::applications::transfer::denom::{Amount, RawCoin};

use crate::error::{handle_generic_error, Error};
use crate::ibc::denom::{derive_ibc_denom, Denom, TaggedDenom, TaggedDenomRef};
use crate::types::id::{TaggedChannelIdRef, TaggedPortIdRef};
use crate::types::tagged::MonoTagged;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Token {
    pub denom: Denom,
    pub amount: Amount,
}

pub type TaggedToken<Chain> = MonoTagged<Chain, Token>;
pub type TaggedTokenRef<'a, Chain> = MonoTagged<Chain, &'a Token>;

pub trait TaggedTokenExt<Chain> {
    fn denom(&self) -> TaggedDenomRef<Chain>;

    fn amount(&self) -> Amount;

    fn transfer<Counterparty>(
        &self,
        port_id: &TaggedPortIdRef<Counterparty, Chain>,
        channel_id: &TaggedChannelIdRef<Counterparty, Chain>,
    ) -> Result<TaggedToken<Counterparty>, Error>;
}

pub trait TaggedDenomExt<Chain> {
    fn with_amount(&self, amount: impl Into<Amount>) -> TaggedToken<Chain>;
}

impl Token {
    pub fn new(denom: Denom, amount: impl Into<Amount>) -> Self {
        Self {
            denom,
            amount: amount.into(),
        }
    }

    pub fn as_coin(&self) -> RawCoin {
        RawCoin {
            denom: self.denom.to_string(),
            amount: self.amount,
        }
    }
}

impl<Chain> TaggedTokenExt<Chain> for TaggedToken<Chain> {
    fn denom(&self) -> TaggedDenomRef<Chain> {
        self.map_ref(|t| &t.denom)
    }

    fn amount(&self) -> Amount {
        self.value().amount
    }

    fn transfer<Counterparty>(
        &self,
        port_id: &TaggedPortIdRef<Counterparty, Chain>,
        channel_id: &TaggedChannelIdRef<Counterparty, Chain>,
    ) -> Result<TaggedToken<Counterparty>, Error> {
        let denom = derive_ibc_denom(port_id, channel_id, &self.denom())?;

        Ok(denom.with_amount(self.value().amount))
    }
}

impl<'a, Chain> TaggedTokenExt<Chain> for TaggedTokenRef<'a, Chain> {
    fn denom(&self) -> TaggedDenomRef<Chain> {
        self.map_ref(|t| &t.denom)
    }

    fn amount(&self) -> Amount {
        self.value().amount
    }

    fn transfer<Counterparty>(
        &self,
        port_id: &TaggedPortIdRef<Counterparty, Chain>,
        channel_id: &TaggedChannelIdRef<Counterparty, Chain>,
    ) -> Result<TaggedToken<Counterparty>, Error> {
        let denom = derive_ibc_denom(port_id, channel_id, &self.denom())?;

        Ok(denom.with_amount(self.value().amount))
    }
}

impl<Chain> TaggedDenomExt<Chain> for TaggedDenom<Chain> {
    fn with_amount(&self, amount: impl Into<Amount>) -> TaggedToken<Chain> {
        self.map(|denom| Token {
            denom: denom.clone(),
            amount: amount.into(),
        })
    }
}

impl<'a, Chain> TaggedDenomExt<Chain> for TaggedDenomRef<'a, Chain> {
    fn with_amount(&self, amount: impl Into<Amount>) -> TaggedToken<Chain> {
        self.map(|denom| Token {
            denom: (*denom).clone(),
            amount: amount.into(),
        })
    }
}

impl<I: Into<Amount>> Add<I> for Token {
    type Output = Self;

    fn add(self, amount: I) -> Self {
        Self {
            denom: self.denom,
            amount: self.amount.checked_add(amount).unwrap(),
        }
    }
}

impl<I: Into<Amount>> Sub<I> for Token {
    type Output = Self;

    fn sub(self, amount: I) -> Self {
        Self {
            denom: self.denom,
            amount: self.amount.checked_sub(amount).unwrap(),
        }
    }
}

impl<Chain, I: Into<Amount>> Add<I> for MonoTagged<Chain, Token> {
    type Output = Self;

    fn add(self, amount: I) -> Self {
        self.map_into(|t| t + amount.into())
    }
}

impl<Chain, I: Into<Amount>> Sub<I> for MonoTagged<Chain, Token> {
    type Output = Self;

    fn sub(self, amount: I) -> Self {
        self.map_into(|t| t - amount.into())
    }
}

impl Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.amount, self.denom)
    }
}

impl TryFrom<RawCoin> for Token {
    type Error = Error;

    fn try_from(fee: RawCoin) -> Result<Self, Error> {
        let denom = Denom::base(&fee.denom);
        let amount =
            u128::try_from(fee.amount.0).map_err(|e| handle_generic_error(eyre!("{}", e)))?;

        Ok(Token::new(denom, amount))
    }
}
