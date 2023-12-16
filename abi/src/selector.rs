//! Zink ABI utils
#![cfg(feature = "selector")]

use crate::Abi;
use sha3::{Digest, Keccak256};

/// Generate a keccak hash of the input (sha3)
pub fn keccak256(input: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(input);
    hasher.finalize().into()
}

/// Parse selector from bytes.
pub fn parse(bytes: &[u8]) -> [u8; 4] {
    let mut selector = [0u8; 4];
    selector.copy_from_slice(&keccak256(bytes)[..4]);
    selector
}

impl Abi {
    /// Get function signature.
    pub fn signature(&self) -> String {
        self.name.clone()
            + "("
            + &self
                .inputs
                .iter()
                .map(|i| i.ty.as_ref())
                .collect::<Vec<_>>()
                .join(",")
            + ")"
    }

    /// Get function selector.
    pub fn selector(&self) -> [u8; 4] {
        parse(self.signature().as_bytes())
    }
}
