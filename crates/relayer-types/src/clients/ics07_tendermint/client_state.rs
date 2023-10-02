use std::convert::{TryFrom, TryInto};
use std::time::Duration;

use prost::Message;
use serde::{Deserialize, Serialize};

use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::core::client::v1::Height as RawHeight;
use ibc_proto::ibc::lightclients::tendermint::v1::ClientState as RawTmClientState;
use ibc_proto::protobuf::Protobuf;

use tendermint_light_client_verifier::options::Options;

use crate::clients::ics07_tendermint::error::Error;
use crate::clients::ics07_tendermint::header::Header as TmHeader;
use crate::core::ics02_client::client_state::{
    ClientState as Ics2ClientState, UpgradableClientState,
};
use crate::core::ics02_client::client_type::ClientType;
use crate::core::ics02_client::error::Error as Ics02Error;
use crate::core::ics02_client::trust_threshold::TrustThreshold;
use crate::core::ics23_commitment::specs::ProofSpecs;
use crate::core::ics24_host::identifier::ChainId;
use crate::timestamp::{Timestamp, ZERO_DURATION};
use crate::Height;

pub const TENDERMINT_CLIENT_STATE_TYPE_URL: &str = "/ibc.lightclients.tendermint.v1.ClientState";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientState {
    pub chain_id: ChainId,
    pub trust_threshold: TrustThreshold,
    pub trusting_period: Duration,
    pub unbonding_period: Duration,
    pub max_clock_drift: Duration,
    pub latest_height: Height,
    pub proof_specs: ProofSpecs,
    pub upgrade_path: Vec<String>,
    pub allow_update: AllowUpdate,
    pub frozen_height: Option<Height>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowUpdate {
    pub after_expiry: bool,
    pub after_misbehaviour: bool,
}

impl ClientState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        chain_id: ChainId,
        trust_threshold: TrustThreshold,
        trusting_period: Duration,
        unbonding_period: Duration,
        max_clock_drift: Duration,
        latest_height: Height,
        proof_specs: ProofSpecs,
        upgrade_path: Vec<String>,
        allow_update: AllowUpdate,
    ) -> Result<ClientState, Error> {
        // Basic validation of trusting period and unbonding period: each should be non-zero.
        if trusting_period <= Duration::new(0, 0) {
            return Err(Error::invalid_trusting_period(format!(
                "ClientState trusting period ({trusting_period:?}) must be greater than zero"
            )));
        }

        if unbonding_period <= Duration::new(0, 0) {
            return Err(Error::invalid_unbonding_period(format!(
                "ClientState unbonding period ({unbonding_period:?}) must be greater than zero"
            )));
        }

        if trusting_period >= unbonding_period {
            return Err(Error::invalid_trusting_period(format!(
                "ClientState trusting period ({trusting_period:?}) must be smaller than unbonding period ({unbonding_period:?})",
            )));
        }

        // `TrustThreshold` is guaranteed to be in the range `[0, 1)`,
        // but a zero value is invalid in this context.
        if trust_threshold.numerator() == 0 {
            return Err(Error::validation(
                "ClientState trust threshold cannot be zero".to_string(),
            ));
        }

        // Dividing by zero is undefined so we also rule out a zero denominator.
        // This should be checked already by the `TrustThreshold` constructor
        // but it does not hurt to redo the check here.
        if trust_threshold.denominator() == 0 {
            return Err(Error::validation(
                "ClientState trust threshold cannot divide by zero".to_string(),
            ));
        }

        // Disallow empty proof-specs
        if proof_specs.is_empty() {
            return Err(Error::validation(
                "ClientState proof specs cannot be empty".to_string(),
            ));
        }

        Ok(Self {
            chain_id,
            trust_threshold,
            trusting_period,
            unbonding_period,
            max_clock_drift,
            latest_height,
            proof_specs,
            upgrade_path,
            allow_update,
            frozen_height: None,
        })
    }

    pub fn latest_height(&self) -> Height {
        self.latest_height
    }

    pub fn with_header(self, h: TmHeader) -> Result<Self, Error> {
        Ok(ClientState {
            latest_height: Height::new(
                self.latest_height.revision_number(),
                h.signed_header.header.height.into(),
            )
            .map_err(|_| Error::invalid_header_height(h.signed_header.header.height.value()))?,
            ..self
        })
    }

    pub fn with_frozen_height(self, h: Height) -> Result<Self, Error> {
        Ok(Self {
            frozen_height: Some(h),
            ..self
        })
    }

    /// Get the refresh time to ensure the state does not expire
    pub fn refresh_time(&self) -> Option<Duration> {
        Some(2 * self.trusting_period / 3)
    }

    /// Helper method to produce a [`Options`] struct for use in
    /// Tendermint-specific light client verification.
    pub fn as_light_client_options(&self) -> Options {
        Options {
            trust_threshold: self.trust_threshold.into(),
            trusting_period: self.trusting_period,
            clock_drift: self.max_clock_drift,
        }
    }

    /// Verify the time and height delays
    pub fn verify_delay_passed(
        current_time: Timestamp,
        current_height: Height,
        processed_time: Timestamp,
        processed_height: Height,
        delay_period_time: Duration,
        delay_period_blocks: u64,
    ) -> Result<(), Error> {
        let earliest_time =
            (processed_time + delay_period_time).map_err(Error::timestamp_overflow)?;
        if !(current_time == earliest_time || current_time.after(&earliest_time)) {
            return Err(Error::not_enough_time_elapsed(current_time, earliest_time));
        }

        let earliest_height = processed_height + delay_period_blocks;
        if current_height < earliest_height {
            return Err(Error::not_enough_blocks_elapsed(
                current_height,
                earliest_height,
            ));
        }

        Ok(())
    }

    /// Verify that the client is at a sufficient height and unfrozen at the given height
    pub fn verify_height(&self, height: Height) -> Result<(), Error> {
        if self.latest_height < height {
            return Err(Error::insufficient_height(self.latest_height(), height));
        }

        match self.frozen_height {
            Some(frozen_height) if frozen_height <= height => {
                Err(Error::client_frozen(frozen_height, height))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeOptions {
    pub unbonding_period: Duration,
}

impl UpgradableClientState for ClientState {
    type UpgradeOptions = UpgradeOptions;

    fn upgrade(
        &mut self,
        upgrade_height: Height,
        upgrade_options: UpgradeOptions,
        chain_id: ChainId,
    ) {
        // Reset custom fields to zero values
        self.trusting_period = ZERO_DURATION;
        self.trust_threshold = TrustThreshold::CLIENT_STATE_RESET;
        self.allow_update.after_expiry = false;
        self.allow_update.after_misbehaviour = false;
        self.frozen_height = None;
        self.max_clock_drift = ZERO_DURATION;

        // Upgrade the client state
        self.latest_height = upgrade_height;
        self.unbonding_period = upgrade_options.unbonding_period;
        self.chain_id = chain_id;
    }
}

impl Ics2ClientState for ClientState {
    fn chain_id(&self) -> ChainId {
        self.chain_id.clone()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Tendermint
    }

    fn latest_height(&self) -> Height {
        self.latest_height
    }

    fn frozen_height(&self) -> Option<Height> {
        self.frozen_height
    }

    fn expired(&self, elapsed: Duration) -> bool {
        elapsed > self.trusting_period
    }
}

impl Protobuf<RawTmClientState> for ClientState {}

impl TryFrom<RawTmClientState> for ClientState {
    type Error = Error;

    fn try_from(raw: RawTmClientState) -> Result<Self, Self::Error> {
        let trust_threshold = raw.trust_level.ok_or_else(Error::missing_trust_threshold)?;

        // We need to handle the case where the client is being upgraded and the trust threshold is set to 0/0
        let trust_threshold = if trust_threshold.denominator == 0 && trust_threshold.numerator == 0
        {
            TrustThreshold::CLIENT_STATE_RESET
        } else {
            trust_threshold
                .try_into()
                .map_err(|e| Error::invalid_trust_threshold(format!("{e}")))?
        };

        // In `RawClientState`, a `frozen_height` of `0` means "not frozen".
        // See:
        // https://github.com/cosmos/ibc-go/blob/8422d0c4c35ef970539466c5bdec1cd27369bab3/modules/light-clients/07-tendermint/types/client_state.go#L74
        let frozen_height = raw
            .frozen_height
            .and_then(|raw_height| raw_height.try_into().ok());

        #[allow(deprecated)]
        Ok(Self {
            chain_id: ChainId::from_string(raw.chain_id.as_str()),
            trust_threshold,
            trusting_period: raw
                .trusting_period
                .ok_or_else(Error::missing_trusting_period)?
                .try_into()
                .map_err(|_| Error::negative_trusting_period())?,
            unbonding_period: raw
                .unbonding_period
                .ok_or_else(Error::missing_unbonding_period)?
                .try_into()
                .map_err(|_| Error::negative_unbonding_period())?,
            max_clock_drift: raw
                .max_clock_drift
                .ok_or_else(Error::missing_max_clock_drift)?
                .try_into()
                .map_err(|_| Error::negative_max_clock_drift())?,
            latest_height: raw
                .latest_height
                .ok_or_else(Error::missing_latest_height)?
                .try_into()
                .map_err(|_| Error::missing_latest_height())?,
            frozen_height,
            upgrade_path: raw.upgrade_path,
            allow_update: AllowUpdate {
                after_expiry: raw.allow_update_after_expiry,
                after_misbehaviour: raw.allow_update_after_misbehaviour,
            },
            proof_specs: raw.proof_specs.into(),
        })
    }
}

impl From<ClientState> for RawTmClientState {
    fn from(value: ClientState) -> Self {
        #[allow(deprecated)]
        Self {
            chain_id: value.chain_id.to_string(),
            trust_level: Some(value.trust_threshold.into()),
            trusting_period: Some(value.trusting_period.into()),
            unbonding_period: Some(value.unbonding_period.into()),
            max_clock_drift: Some(value.max_clock_drift.into()),
            frozen_height: Some(value.frozen_height.map(|height| height.into()).unwrap_or(
                RawHeight {
                    revision_number: 0,
                    revision_height: 0,
                },
            )),
            latest_height: Some(value.latest_height.into()),
            proof_specs: value.proof_specs.into(),
            upgrade_path: value.upgrade_path,
            allow_update_after_expiry: value.allow_update.after_expiry,
            allow_update_after_misbehaviour: value.allow_update.after_misbehaviour,
        }
    }
}

impl Protobuf<Any> for ClientState {}

impl TryFrom<Any> for ClientState {
    type Error = Ics02Error;

    fn try_from(raw: Any) -> Result<Self, Self::Error> {
        use bytes::Buf;
        use core::ops::Deref;

        fn decode_client_state<B: Buf>(buf: B) -> Result<ClientState, Error> {
            RawTmClientState::decode(buf)
                .map_err(Error::decode)?
                .try_into()
        }

        match raw.type_url.as_str() {
            TENDERMINT_CLIENT_STATE_TYPE_URL => {
                decode_client_state(raw.value.deref()).map_err(Into::into)
            }
            _ => Err(Ics02Error::unknown_client_state_type(raw.type_url)),
        }
    }
}

impl From<ClientState> for Any {
    fn from(client_state: ClientState) -> Self {
        Any {
            type_url: TENDERMINT_CLIENT_STATE_TYPE_URL.to_string(),
            value: Protobuf::<RawTmClientState>::encode_vec(&client_state),
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::Height;
    use core::time::Duration;
    use test_log::test;

    use ibc_proto::ics23::ProofSpec as Ics23ProofSpec;
    use tendermint_rpc::endpoint::abci_query::AbciQuery;

    use crate::clients::ics07_tendermint::client_state::{AllowUpdate, ClientState};
    use crate::core::ics02_client::trust_threshold::TrustThreshold;
    use crate::core::ics23_commitment::specs::ProofSpecs;
    use crate::core::ics24_host::identifier::ChainId;
    use crate::test::test_serialization_roundtrip;
    use crate::timestamp::{Timestamp, ZERO_DURATION};

    #[derive(Clone, Debug, PartialEq)]
    struct ClientStateParams {
        id: ChainId,
        trust_threshold: TrustThreshold,
        trusting_period: Duration,
        unbonding_period: Duration,
        max_clock_drift: Duration,
        latest_height: Height,
        proof_specs: ProofSpecs,
        upgrade_path: Vec<String>,
        allow_update: AllowUpdate,
    }

    #[test]
    fn serialization_roundtrip_no_proof() {
        let json_data =
            include_str!("../../../tests/support/query/serialization/client_state.json");
        test_serialization_roundtrip::<AbciQuery>(json_data);
    }

    #[test]
    fn serialization_roundtrip_with_proof() {
        let json_data =
            include_str!("../../../tests/support/query/serialization/client_state_proof.json");
        test_serialization_roundtrip::<AbciQuery>(json_data);
    }

    #[test]
    fn client_state_new() {
        // Define a "default" set of parameters to reuse throughout these tests.
        let default_params: ClientStateParams = ClientStateParams {
            id: ChainId::default(),
            trust_threshold: TrustThreshold::TWO_THIRDS,
            trusting_period: Duration::new(64000, 0),
            unbonding_period: Duration::new(128000, 0),
            max_clock_drift: Duration::new(3, 0),
            latest_height: Height::new(0, 10).unwrap(),
            proof_specs: ProofSpecs::default(),
            upgrade_path: vec!["".to_string()],
            allow_update: AllowUpdate {
                after_expiry: false,
                after_misbehaviour: false,
            },
        };

        struct Test {
            name: String,
            params: ClientStateParams,
            want_pass: bool,
        }

        let tests: Vec<Test> = vec![
            Test {
                name: "Valid parameters".to_string(),
                params: default_params.clone(),
                want_pass: true,
            },
            Test {
                name: "Invalid unbonding period".to_string(),
                params: ClientStateParams {
                    unbonding_period: ZERO_DURATION,
                    ..default_params.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Invalid (too small) trusting period".to_string(),
                params: ClientStateParams {
                    trusting_period: ZERO_DURATION,
                    ..default_params.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Invalid (too large) trusting period w.r.t. unbonding period".to_string(),
                params: ClientStateParams {
                    trusting_period: Duration::new(11, 0),
                    unbonding_period: Duration::new(10, 0),
                    ..default_params.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Invalid (zero) trust threshold".to_string(),
                params: ClientStateParams {
                    trust_threshold: TrustThreshold::new(0, 3).unwrap(),
                    ..default_params.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Invalid (empty) proof specs".to_string(),
                params: ClientStateParams {
                    proof_specs: ProofSpecs::from(Vec::<Ics23ProofSpec>::new()),
                    ..default_params
                },
                want_pass: false,
            },
        ]
        .into_iter()
        .collect();

        for test in tests {
            let p = test.params.clone();

            let cs_result = ClientState::new(
                p.id,
                p.trust_threshold,
                p.trusting_period,
                p.unbonding_period,
                p.max_clock_drift,
                p.latest_height,
                p.proof_specs,
                p.upgrade_path,
                p.allow_update,
            );

            assert_eq!(
                test.want_pass,
                cs_result.is_ok(),
                "ClientState::new() failed for test {}, \nmsg{:?} with error {:?}",
                test.name,
                test.params.clone(),
                cs_result.err(),
            );
        }
    }

    #[test]
    fn client_state_verify_delay_passed() {
        #[derive(Debug, Clone)]
        struct Params {
            current_time: Timestamp,
            current_height: Height,
            processed_time: Timestamp,
            processed_height: Height,
            delay_period_time: Duration,
            delay_period_blocks: u64,
        }
        struct Test {
            name: String,
            params: Params,
            want_pass: bool,
        }
        let now = Timestamp::now();

        let tests: Vec<Test> = vec![
            Test {
                name: "Successful delay verification".to_string(),
                params: Params {
                    current_time: (now + Duration::from_nanos(2000)).unwrap(),
                    current_height: Height::new(0, 5).unwrap(),
                    processed_time: (now + Duration::from_nanos(1000)).unwrap(),
                    processed_height: Height::new(0, 3).unwrap(),
                    delay_period_time: Duration::from_nanos(500),
                    delay_period_blocks: 2,
                },
                want_pass: true,
            },
            Test {
                name: "Delay period(time) has not elapsed".to_string(),
                params: Params {
                    current_time: (now + Duration::from_nanos(1200)).unwrap(),
                    current_height: Height::new(0, 5).unwrap(),
                    processed_time: (now + Duration::from_nanos(1000)).unwrap(),
                    processed_height: Height::new(0, 3).unwrap(),
                    delay_period_time: Duration::from_nanos(500),
                    delay_period_blocks: 2,
                },
                want_pass: false,
            },
            Test {
                name: "Delay period(blocks) has not elapsed".to_string(),
                params: Params {
                    current_time: (now + Duration::from_nanos(2000)).unwrap(),
                    current_height: Height::new(0, 5).unwrap(),
                    processed_time: (now + Duration::from_nanos(1000)).unwrap(),
                    processed_height: Height::new(0, 4).unwrap(),
                    delay_period_time: Duration::from_nanos(500),
                    delay_period_blocks: 2,
                },
                want_pass: false,
            },
        ];

        for test in tests {
            let res = ClientState::verify_delay_passed(
                test.params.current_time,
                test.params.current_height,
                test.params.processed_time,
                test.params.processed_height,
                test.params.delay_period_time,
                test.params.delay_period_blocks,
            );

            assert_eq!(
                test.want_pass,
                res.is_ok(),
                "ClientState::verify_delay_passed() failed for test {}, \nmsg{:?} with error {:?}",
                test.name,
                test.params.clone(),
                res.err(),
            );
        }
    }

    #[test]
    fn client_state_verify_height() {
        // Define a "default" set of parameters to reuse throughout these tests.
        let default_params: ClientStateParams = ClientStateParams {
            id: ChainId::default(),
            trust_threshold: TrustThreshold::TWO_THIRDS,
            trusting_period: Duration::new(64000, 0),
            unbonding_period: Duration::new(128000, 0),
            max_clock_drift: Duration::new(3, 0),
            latest_height: Height::new(1, 10).unwrap(),
            proof_specs: ProofSpecs::default(),
            upgrade_path: vec!["".to_string()],
            allow_update: AllowUpdate {
                after_expiry: false,
                after_misbehaviour: false,
            },
        };

        struct Test {
            name: String,
            height: Height,
            setup: Option<Box<dyn FnOnce(ClientState) -> ClientState>>,
            want_pass: bool,
        }

        let tests = vec![
            Test {
                name: "Successful height verification".to_string(),
                height: Height::new(1, 8).unwrap(),
                setup: None,
                want_pass: true,
            },
            Test {
                name: "Invalid (too large)  client height".to_string(),
                height: Height::new(1, 12).unwrap(),
                setup: None,
                want_pass: false,
            },
            Test {
                name: "Invalid, client is frozen below current height".to_string(),
                height: Height::new(1, 6).unwrap(),
                setup: Some(Box::new(|client_state| {
                    client_state
                        .with_frozen_height(Height::new(1, 5).unwrap())
                        .unwrap()
                })),
                want_pass: false,
            },
        ];

        for test in tests {
            let p = default_params.clone();
            let client_state = ClientState::new(
                p.id,
                p.trust_threshold,
                p.trusting_period,
                p.unbonding_period,
                p.max_clock_drift,
                p.latest_height,
                p.proof_specs,
                p.upgrade_path,
                p.allow_update,
            )
            .unwrap();
            let client_state = match test.setup {
                Some(setup) => (setup)(client_state),
                _ => client_state,
            };
            let res = client_state.verify_height(test.height);

            assert_eq!(
                test.want_pass,
                res.is_ok(),
                "ClientState::verify_delay_height() failed for test {}, \nmsg{:?} with error {:?}",
                test.name,
                test.height,
                res.err(),
            );
        }
    }
}

#[cfg(any(test, feature = "mocks"))]
pub mod test_util {
    use core::time::Duration;

    use tendermint::block::Header;

    use crate::clients::ics07_tendermint::client_state::{AllowUpdate, ClientState};
    use crate::core::ics02_client::height::Height;
    use crate::core::ics24_host::identifier::ChainId;

    pub fn get_dummy_tendermint_client_state(tm_header: Header) -> ClientState {
        ClientState::new(
            ChainId::from(tm_header.chain_id.clone()),
            Default::default(),
            Duration::from_secs(64000),
            Duration::from_secs(128000),
            Duration::from_millis(3000),
            Height::new(
                ChainId::chain_version(tm_header.chain_id.as_str()),
                u64::from(tm_header.height),
            )
            .unwrap(),
            Default::default(),
            vec!["".to_string()],
            AllowUpdate {
                after_expiry: false,
                after_misbehaviour: false,
            },
        )
        .unwrap()
    }
}
