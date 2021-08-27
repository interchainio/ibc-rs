use flex_error::{define_error, DisplayOnly, TraceError};
use std::io::Error as IoError;

define_error! {
    Error {
        InvalidKey
            [ TraceError<signature::Error> ]
            |_| { "invalid key: could not build signing key from private key bytes" },

        InvalidKeyRaw
            [ TraceError<bitcoin::secp256k1::Error> ]
            |_| { "invalid key: could not build signing key from private key bytes" },

        KeyNotFound
            |_| { "key not found" },

        KeyAlreadyExist
            |_| { "key already exist" },

        InvalidMnemonic
            [ DisplayOnly<anyhow::Error> ]
            |_| { "invalid mnemonic" },

        PrivateKey
            [ TraceError<bitcoin::util::bip32::Error> ]
            |_| { "cannot generate private key" },

        UnsupportedPublicKey
            { key_type: String }
            |e| {
                format!("unsupported public key: {}. only secp256k1 pub keys are currently supported",
                    e.key_type)
            },

        EncodedPublicKey
            {
                key: String,
            }
            [ TraceError<serde_json::Error> ]
            |e| {
                format!("cannot deserialize the encoded public key {0}",
                    e.key)
            },

        Bech32Account
            [ TraceError<bech32::Error> ]
            |_| { "cannot generate bech32 account" },

        Bech32
            [ TraceError<bech32::Error> ]
            |_| { "bech32 error" },

        PublicKeyMismatch
             { keyfile: Vec<u8>, mnemonic: Vec<u8> }
            |_| { "mismatch between the public key in the key file and the public key in the mnemonic" },

        KeyFileEncode
            { file_path: String }
            [ TraceError<serde_json::Error> ]
            |e| {
                format!("error encoding key file at '{}'",
                    e.file_path)
            },

        Encode
            [ TraceError<serde_json::Error> ]
            |_| { "error encoding key" },

        KeyFileDecode
            { file_path: String }
            [ TraceError<serde_json::Error> ]
            |e| {
                format!("error decoding key file at '{}'",
                    e.file_path)
            },

        KeyFileIo
            {
                file_path: String,
                description: String,
            }
            [ TraceError<IoError> ]
            |e| {
                format!("I/O error on key file at '{}': {}",
                    e.file_path, e.description)
            },

        KeyFileNotFound
            { file_path: String }
            |e| {
                format!("cannot find key file at '{}'",
                    e.file_path)
            },

        HomeLocationUnavailable
            |_| { "home location is unavailable" },

        RemoveIoFail
            {
                file_path: String,
            }
            [ TraceError<IoError> ]
            |e| {
                format!("I/O error while removing key file at location '{}'",
                    e.file_path)
            },

        InvalidHdPath
            {
                path: String,
            }
            |e| {
                format!("invalid HD path: {0}", e.path)
            },
    }
}
