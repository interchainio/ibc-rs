use alloc::collections::btree_map::BTreeMap as HashMap;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use bech32::{ToBase32, Variant};
use bip39::{Language, Mnemonic, Seed};
use bitcoin::{
    network::constants::Network,
    secp256k1::Secp256k1,
    util::bip32::{DerivationPath, ExtendedPrivKey, ExtendedPubKey},
};
use hdpath::StandardHDPath;
use ibc::ics24_host::identifier::ChainId;
use k256::ecdsa::{signature::Signer, Signature, SigningKey};
use ripemd160::Ripemd160;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use errors::Error;
pub use pub_key::EncodedPubKey;

pub mod errors;
mod pub_key;

pub type HDPath = StandardHDPath;

pub const KEYSTORE_DEFAULT_FOLDER: &str = ".hermes/keys/";
pub const KEYSTORE_DISK_BACKEND: &str = "keyring-test";
pub const KEYSTORE_FILE_EXTENSION: &str = "json";

// /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\
// WARNING: Changing this struct in backward incompatible way
//          will force users to re-import their keys.
// /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\ /!\
/// Key entry stores the Private Key and Public Key as well the address
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEntry {
    /// Public key
    pub public_key: ExtendedPubKey,

    /// Private key
    pub private_key: ExtendedPrivKey,

    /// Account Bech32 format - TODO allow hrp
    pub account: String,

    /// Address
    pub address: Vec<u8>,
}

/// JSON key seed file
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyFile {
    pub name: String,
    pub r#type: String,
    pub address: String,
    pub pubkey: String,
    pub mnemonic: String,
}

impl KeyEntry {
    fn from_key_file(key_file: KeyFile, hd_path: &HDPath) -> Result<Self, Error> {
        // Decode the Bech32-encoded address from the key file
        let keyfile_address_bytes = decode_bech32(&key_file.address)?;

        let encoded_key: EncodedPubKey = key_file.pubkey.parse()?;
        let mut keyfile_pubkey_bytes = encoded_key.into_bytes();

        // Decode the private key from the mnemonic
        let private_key = private_key_from_mnemonic(&key_file.mnemonic, hd_path)?;
        let derived_pubkey = ExtendedPubKey::from_private(&Secp256k1::new(), &private_key);
        let derived_pubkey_bytes = derived_pubkey.public_key.to_bytes();
        assert!(derived_pubkey_bytes.len() <= keyfile_pubkey_bytes.len());

        // FIXME: For some reason that is currently unclear, the public key decoded from
        //        the keyfile contains a few extraneous leading bytes. To compare both
        //        public keys, we therefore strip those leading bytes off and keep the
        //        common parts.
        let keyfile_pubkey_bytes =
            keyfile_pubkey_bytes.split_off(keyfile_pubkey_bytes.len() - derived_pubkey_bytes.len());

        // Ensure that the public key in the key file and the one extracted from the mnemonic match.
        if keyfile_pubkey_bytes != derived_pubkey_bytes {
            Err(Error::public_key_mismatch(
                keyfile_pubkey_bytes,
                derived_pubkey_bytes,
            ))
        } else {
            Ok(Self {
                public_key: derived_pubkey,
                private_key,
                account: key_file.address,
                address: keyfile_address_bytes,
            })
        }
    }
}

pub trait KeyStore {
    fn get_key(&self, key_name: &str) -> Result<KeyEntry, Error>;
    fn add_key(&mut self, key_name: &str, key_entry: KeyEntry) -> Result<(), Error>;
    fn remove_key(&mut self, key_name: &str) -> Result<(), Error>;
    fn keys(&self) -> Result<Vec<(String, KeyEntry)>, Error>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Memory {
    account_prefix: String,
    keys: HashMap<String, KeyEntry>,
}

impl Memory {
    pub fn new(account_prefix: String) -> Self {
        Self {
            account_prefix,
            keys: HashMap::new(),
        }
    }
}

impl KeyStore for Memory {
    fn get_key(&self, key_name: &str) -> Result<KeyEntry, Error> {
        self.keys
            .get(key_name)
            .cloned()
            .ok_or_else(Error::key_not_found)
    }

    fn add_key(&mut self, key_name: &str, key_entry: KeyEntry) -> Result<(), Error> {
        if self.keys.contains_key(key_name) {
            Err(Error::key_already_exist())
        } else {
            self.keys.insert(key_name.to_string(), key_entry);

            Ok(())
        }
    }

    fn remove_key(&mut self, key_name: &str) -> Result<(), Error> {
        self.keys
            .remove(key_name)
            .ok_or_else(Error::key_not_found)?;

        Ok(())
    }

    fn keys(&self) -> Result<Vec<(String, KeyEntry)>, Error> {
        Ok(self
            .keys
            .iter()
            .map(|(n, k)| (n.to_string(), k.clone()))
            .collect())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Test {
    account_prefix: String,
    store: PathBuf,
}

impl Test {
    pub fn new(account_prefix: String, store: PathBuf) -> Self {
        Self {
            account_prefix,
            store,
        }
    }
}

impl KeyStore for Test {
    fn get_key(&self, key_name: &str) -> Result<KeyEntry, Error> {
        let mut key_file = self.store.join(key_name);
        key_file.set_extension(KEYSTORE_FILE_EXTENSION);

        if !key_file.as_path().exists() {
            return Err(Error::key_file_not_found(format!("{}", key_file.display())));
        }

        let file = File::open(&key_file).map_err(|e| {
            Error::key_file_io(
                key_file.display().to_string(),
                "failed to open file".to_string(),
                e,
            )
        })?;

        let key_entry = serde_json::from_reader(file)
            .map_err(|e| Error::key_file_decode(format!("{}", key_file.display()), e))?;

        Ok(key_entry)
    }

    fn add_key(&mut self, key_name: &str, key_entry: KeyEntry) -> Result<(), Error> {
        let mut filename = self.store.join(key_name);
        filename.set_extension(KEYSTORE_FILE_EXTENSION);
        let file_path = filename.display().to_string();

        let file = File::create(filename).map_err(|e| {
            Error::key_file_io(file_path.clone(), "failed to create file".to_string(), e)
        })?;

        serde_json::to_writer_pretty(file, &key_entry)
            .map_err(|e| Error::key_file_encode(file_path, e))?;

        Ok(())
    }

    fn remove_key(&mut self, key_name: &str) -> Result<(), Error> {
        let mut filename = self.store.join(key_name);
        filename.set_extension(KEYSTORE_FILE_EXTENSION);

        fs::remove_file(filename.clone())
            .map_err(|e| Error::remove_io_fail(filename.display().to_string(), e))?;

        Ok(())
    }

    fn keys(&self) -> Result<Vec<(String, KeyEntry)>, Error> {
        let dir = fs::read_dir(&self.store).map_err(|e| {
            Error::key_file_io(
                self.store.display().to_string(),
                "failed to list keys".to_string(),
                e,
            )
        })?;

        let ext = OsStr::new(KEYSTORE_FILE_EXTENSION);

        dir.into_iter()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension() == Some(ext))
            .flat_map(|path| path.file_stem().map(OsStr::to_owned))
            .flat_map(|stem| stem.to_str().map(ToString::to_string))
            .map(|name| self.get_key(&name).map(|key| (name, key)))
            .collect()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Store {
    Memory,
    Test,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum KeyRing {
    Memory(Memory),
    Test(Test),
}

impl KeyRing {
    pub fn new(store: Store, account_prefix: &str, chain_id: &ChainId) -> Result<Self, Error> {
        match store {
            Store::Memory => Ok(Self::Memory(Memory::new(account_prefix.to_string()))),

            Store::Test => {
                let keys_folder = disk_store_path(chain_id.as_str())?;

                // Create keys folder if it does not exist
                fs::create_dir_all(&keys_folder).map_err(|e| {
                    Error::key_file_io(
                        keys_folder.display().to_string(),
                        "failed to create keys folder".to_string(),
                        e,
                    )
                })?;

                Ok(Self::Test(Test::new(
                    account_prefix.to_string(),
                    keys_folder,
                )))
            }
        }
    }

    pub fn get_key(&self, key_name: &str) -> Result<KeyEntry, Error> {
        match self {
            KeyRing::Memory(m) => m.get_key(key_name),
            KeyRing::Test(d) => d.get_key(key_name),
        }
    }

    pub fn add_key(&mut self, key_name: &str, key_entry: KeyEntry) -> Result<(), Error> {
        match self {
            KeyRing::Memory(m) => m.add_key(key_name, key_entry),
            KeyRing::Test(d) => d.add_key(key_name, key_entry),
        }
    }

    pub fn remove_key(&mut self, key_name: &str) -> Result<(), Error> {
        match self {
            KeyRing::Memory(m) => m.remove_key(key_name),
            KeyRing::Test(d) => d.remove_key(key_name),
        }
    }

    pub fn keys(&self) -> Result<Vec<(String, KeyEntry)>, Error> {
        match self {
            KeyRing::Memory(m) => m.keys(),
            KeyRing::Test(d) => d.keys(),
        }
    }

    /// Get key from seed file
    pub fn key_from_seed_file(
        &self,
        key_file_content: &str,
        hd_path: &HDPath,
    ) -> Result<KeyEntry, Error> {
        let key_file: KeyFile = serde_json::from_str(key_file_content).map_err(Error::encode)?;

        KeyEntry::from_key_file(key_file, hd_path)
    }

    /// Add a key entry in the store using a mnemonic.
    pub fn key_from_mnemonic(
        &self,
        mnemonic_words: &str,
        hd_path: &HDPath,
    ) -> Result<KeyEntry, Error> {
        // Get the private key from the mnemonic
        let private_key = private_key_from_mnemonic(mnemonic_words, hd_path)?;

        // Get the public Key from the private key
        let public_key = ExtendedPubKey::from_private(&Secp256k1::new(), &private_key);

        // Get address from the public Key
        let address = get_address(public_key);

        // Compute Bech32 account
        let account = bech32::encode(self.account_prefix(), address.to_base32(), Variant::Bech32)
            .map_err(Error::bech32)?;

        Ok(KeyEntry {
            public_key,
            private_key,
            account,
            address,
        })
    }

    /// Sign a message
    pub fn sign_msg(&self, key_name: &str, msg: Vec<u8>) -> Result<Vec<u8>, Error> {
        let key = self.get_key(key_name)?;

        let private_key_bytes = key.private_key.private_key.to_bytes();
        let signing_key =
            SigningKey::from_bytes(private_key_bytes.as_slice()).map_err(Error::invalid_key)?;

        let signature: Signature = signing_key.sign(&msg);
        Ok(signature.as_ref().to_vec())
    }

    pub fn account_prefix(&self) -> &str {
        match self {
            KeyRing::Memory(m) => &m.account_prefix,
            KeyRing::Test(d) => &d.account_prefix,
        }
    }
}

/// Decode an extended private key from a mnemonic
fn private_key_from_mnemonic(
    mnemonic_words: &str,
    hd_path: &StandardHDPath,
) -> Result<ExtendedPrivKey, Error> {
    let mnemonic = Mnemonic::from_phrase(mnemonic_words, Language::English)
        .map_err(Error::invalid_mnemonic)?;

    let seed = Seed::new(&mnemonic, "");

    let private_key = ExtendedPrivKey::new_master(Network::Bitcoin, seed.as_bytes())
        .and_then(|k| k.derive_priv(&Secp256k1::new(), &DerivationPath::from(hd_path)))
        .map_err(Error::private_key)?;

    Ok(private_key)
}

/// Return an address from a Public Key
fn get_address(pk: ExtendedPubKey) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(pk.public_key.to_bytes().as_slice());

    // Read hash digest over the public key bytes & consume hasher
    let pk_hash = hasher.finalize();

    // Plug the hash result into the next crypto hash function.
    let mut rip_hasher = Ripemd160::new();
    rip_hasher.update(pk_hash);
    let rip_result = rip_hasher.finalize();

    rip_result.to_vec()
}

fn decode_bech32(input: &str) -> Result<Vec<u8>, Error> {
    use bech32::FromBase32;

    let bytes = bech32::decode(input)
        .and_then(|(_, data, _)| Vec::from_base32(&data))
        .map_err(Error::bech32_account)?;

    Ok(bytes)
}

fn disk_store_path(folder_name: &str) -> Result<PathBuf, Error> {
    let home = dirs_next::home_dir().ok_or_else(Error::home_location_unavailable)?;

    let folder = Path::new(home.as_path())
        .join(KEYSTORE_DEFAULT_FOLDER)
        .join(folder_name)
        .join(KEYSTORE_DISK_BACKEND);

    Ok(folder)
}
