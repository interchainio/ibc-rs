use crate::ics24_host::error::ValidationError;
use crate::prelude::*;
use prost_types::Any;

pub trait Msg: Clone {
    type ValidationError;
    type Raw: From<Self> + prost::Message;

    // TODO: Clarify what is this function supposed to do & its connection to ICS26 routing mod.
    fn route(&self) -> String;

    /// Unique type identifier for this message, to support encoding to/from `prost_types::Any`.
    fn type_url(&self) -> String;

    #[allow(clippy::wrong_self_convention)]
    fn to_any(self) -> Any {
        Any {
            type_url: self.type_url(),
            value: self.get_sign_bytes(),
        }
    }

    fn get_sign_bytes(self) -> Vec<u8> {
        let mut buf = Vec::new();
        let raw_msg: Self::Raw = self.into();
        match prost::Message::encode(&raw_msg, &mut buf) {
            Ok(()) => buf,
            // Severe error that cannot be recovered.
            Err(e) => panic!(
                "Cannot encode the proto message {:?} into a buffer due to underlying error: {}",
                raw_msg, e
            ),
        }
    }

    fn validate_basic(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}
