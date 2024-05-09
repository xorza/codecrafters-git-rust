use std::fmt::Display;
use std::ops::{Index, RangeFrom, RangeTo};
use std::str::FromStr;

use sha1::{Digest, Sha1};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sha1Hash([u8; 20]);

impl Sha1Hash {
    pub fn hash(data: &[u8]) -> Self {
        let mut hasher = Sha1::new();
        hasher.update(data);
        hasher.finalize().into()
    }
}

impl Display for Sha1Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&hex::encode(&self.0))?;

        Ok(())
    }
}

impl From<sha1::digest::Output<Sha1>> for Sha1Hash {
    fn from(output: sha1::digest::Output<Sha1>) -> Self {
        Self(output.into())
    }
}

impl AsRef<[u8]> for Sha1Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Index<usize> for Sha1Hash {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl Index<RangeTo<usize>> for Sha1Hash {
    type Output = [u8];

    fn index(&self, range: RangeTo<usize>) -> &Self::Output {
        &self.0[range]
    }
}

impl Index<RangeFrom<usize>> for Sha1Hash {
    type Output = [u8];

    fn index(&self, range: RangeFrom<usize>) -> &Self::Output {
        &self.0[range]
    }
}

impl FromStr for Sha1Hash {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s)?;
        let mut hash = [0; 20];
        hash.copy_from_slice(&bytes);
        Ok(Sha1Hash(hash))
    }
}

