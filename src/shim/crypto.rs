// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::borrow::Cow;

use super::fvm_shared_latest::{self, commcid::Commitment};
pub use super::fvm_shared_latest::{IPLD_RAW, TICKET_RANDOMNESS_LOOKBACK};
use bls_signatures::{PublicKey as BlsPublicKey, Signature as BlsSignature};
use cid::Cid;
use fvm_ipld_encoding::{
    de,
    repr::{Deserialize_repr, Serialize_repr},
    ser, strict_bytes,
};
use num::FromPrimitive;
use num_derive::FromPrimitive;

/// A cryptographic signature, represented in bytes, of any key protocol.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct Signature {
    pub sig_type: SignatureType,
    pub bytes: Vec<u8>,
}

impl ser::Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let mut bytes = Vec::with_capacity(self.bytes.len() + 1);
        // Insert signature type byte
        bytes.push(self.sig_type as u8);
        bytes.extend_from_slice(&self.bytes);

        strict_bytes::Serialize::serialize(&bytes, serializer)
    }
}

impl<'de> de::Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let bytes: Cow<'de, [u8]> = strict_bytes::Deserialize::deserialize(deserializer)?;
        if bytes.is_empty() {
            return Err(de::Error::custom("Cannot deserialize empty bytes"));
        }

        // Remove signature type byte
        let sig_type = SignatureType::from_u8(bytes[0]).ok_or_else(|| {
            de::Error::custom(format!(
                "Invalid signature type byte (must be 1, 2 or 3), was {}",
                bytes[0]
            ))
        })?;

        Ok(Signature {
            bytes: bytes[1..].to_vec(),
            sig_type,
        })
    }
}

impl Signature {
    pub fn new(sig_type: SignatureType, bytes: Vec<u8>) -> Self {
        Signature { sig_type, bytes }
    }

    /// Creates a BLS Signature given the raw bytes.
    pub fn new_bls(bytes: Vec<u8>) -> Self {
        Self {
            sig_type: SignatureType::Bls,
            bytes,
        }
    }

    /// Creates a SECP Signature given the raw bytes.
    pub fn new_secp256k1(bytes: Vec<u8>) -> Self {
        Self {
            sig_type: SignatureType::Secp256k1,
            bytes,
        }
    }

    pub fn signature_type(&self) -> SignatureType {
        self.sig_type
    }

    /// Checks if a signature is valid given data and address.
    pub fn verify(&self, data: &[u8], addr: &crate::shim::address::Address) -> Result<(), String> {
        use super::fvm_shared_latest::crypto::signature::ops::{
            verify_bls_sig, verify_secp256k1_sig,
        };
        match self.sig_type {
            SignatureType::Bls => verify_bls_sig(&self.bytes, data, addr),
            SignatureType::Secp256k1 => verify_secp256k1_sig(&self.bytes, data, addr),
            SignatureType::Delegated => Ok(()),
        }
    }

    /// Returns reference to signature bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl TryFrom<&Signature> for BlsSignature {
    type Error = anyhow::Error;
    fn try_from(value: &Signature) -> Result<Self, Self::Error> {
        use bls_signatures::Serialize as _;

        match value.sig_type {
            SignatureType::Secp256k1 => {
                anyhow::bail!("cannot convert Secp256k1 signature to bls signature")
            }
            SignatureType::Bls => Ok(BlsSignature::from_bytes(&value.bytes)?),
            SignatureType::Delegated => {
                anyhow::bail!("cannot convert delegated signature to bls signature")
            }
        }
    }
}

// Forest's version of the `verify_bls_aggregate` function is semantically different
// from the version in FVM.
/// Aggregates and verifies BLS signatures collectively.
pub fn verify_bls_aggregate(data: &[&[u8]], pub_keys: &[BlsPublicKey], sig: &Signature) -> bool {
    // If the number of public keys and data does not match, then return false
    if data.len() != pub_keys.len() {
        return false;
    }
    if data.is_empty() {
        return true;
    }

    let bls_sig = match sig.try_into() {
        Ok(bls_sig) => bls_sig,
        _ => return false,
    };

    // Does the aggregate verification
    bls_signatures::verify_messages(&bls_sig, data, pub_keys)
}

/// Returns `String` error if a BLS signature is invalid.
pub fn verify_bls_sig(
    signature: &[u8],
    data: &[u8],
    addr: &crate::shim::address::Address,
) -> Result<(), String> {
    fvm_shared_latest::crypto::signature::ops::verify_bls_sig(signature, data, &addr.into())
}

/// Extracts the raw replica commitment from a CID
/// assuming that it has the correct hashing function and
/// serialization types
pub fn cid_to_replica_commitment_v1(c: &Cid) -> Result<Commitment, &'static str> {
    fvm_shared_latest::commcid::cid_to_replica_commitment_v1(c)
}

/// Signature variants for Filecoin signatures.
#[derive(
    Clone,
    Debug,
    PartialEq,
    FromPrimitive,
    Copy,
    Eq,
    Serialize_repr,
    Deserialize_repr,
    Hash,
    strum::Display,
    strum::EnumString,
)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[repr(u8)]
#[strum(serialize_all = "lowercase")]
pub enum SignatureType {
    Secp256k1 = 1,
    Bls = 2,
    Delegated = 3,
}
