// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use encoding::{de, ser, serde_bytes};
use num_bigint::BigUint;

/// Wrapper around a big int variable to handle token specific functionality
// TODO verify on finished spec whether or not big int or uint
#[derive(Default, Clone, PartialEq, Debug)]
pub struct TokenAmount(pub BigUint);

impl TokenAmount {
    pub fn new(val: u64) -> Self {
        TokenAmount(BigUint::from(val))
    }
}

impl ser::Serialize for TokenAmount {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let bz = self.0.to_bytes_be();
        let value = serde_bytes::Bytes::new(&bz);
        serde_bytes::Serialize::serialize(value, s)
    }
}

impl<'de> de::Deserialize<'de> for TokenAmount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let bz: &[u8] = serde_bytes::Deserialize::deserialize(deserializer)?;
        Ok(TokenAmount(BigUint::from_bytes_be(bz)))
    }
}
