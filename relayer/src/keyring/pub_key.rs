use std::str::FromStr;

use serde::{Deserialize, Deserializer};
use subtle_encoding::base64;
use tracing::{error, trace};

use super::decode_bech32;
use super::errors::Error;

#[derive(Debug)]
pub enum EncodedPubKey {
    Bech32(Vec<u8>),
    Proto(ProtoAny),
}

impl EncodedPubKey {
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            EncodedPubKey::Bech32(vec) => vec,
            EncodedPubKey::Proto(proto) => proto.key,
        }
    }
}

/// A variant of [`EncodedPubKey`].
/// A Protobuf `Any`, having support for deserialization from
/// JSON + base64 (see `deserialize_key`).
#[derive(Debug, Deserialize)]
pub struct ProtoAny {
    #[serde(alias = "@type")]
    tpe: String,

    #[serde(deserialize_with = "deserialize_key")]
    key: Vec<u8>,
}

/// This method is the workhorse for deserializing
/// the `key` field from a public key.
fn deserialize_key<'de, D>(deser: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    // The key is a byte array that is base64-encoded
    // and then marshalled into a JSON String.
    let based64_encoded: Result<String, _> = Deserialize::deserialize(deser);
    let value = base64::decode(based64_encoded?)
        .map_err(|e| serde::de::Error::custom(format!("error in decoding: {}", e)))?;

    Ok(value)
}

impl FromStr for EncodedPubKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Try to deserialize into a JSON Value.
        let maybe_json: Result<ProtoAny, _> = serde_json::from_str(s);

        match maybe_json {
            Ok(proto) => {
                trace!(
                    "deserialized the encoded pub key into a ProtoAny of type '{}'",
                    proto.tpe
                );

                // Ethermint pubkey types:
                // "/ethermint.crypto.v1alpha1.ethsecp256k1.PubKey", "/injective.crypto.v1beta1.ethsecp256k1.PubKey"
                if proto.tpe != "/cosmos.crypto.secp256k1.PubKey"
                    && !proto.tpe.ends_with(".ethsecp256k1.PubKey")
                {
                    Err(Error::unsupported_public_key(proto.tpe))
                } else {
                    Ok(EncodedPubKey::Proto(proto))
                }
            }
            Err(e) if e.classify() == serde_json::error::Category::Syntax => {
                // Input is not syntactically-correct JSON.
                // Attempt to decode via Bech32, for backwards compatibility with the old format.
                trace!("using Bech32 to interpret the encoded pub key '{}'", s);
                Ok(EncodedPubKey::Bech32(decode_bech32(s)?))
            }
            Err(e) => {
                error!(
                    "the encoded pub key is not in a valid format: '{}', error: {}",
                    s, e
                );

                Err(Error::encoded_public_key(s.to_string(), e))
            }
        }
    }
}
