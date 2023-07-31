use serde::{Serialize, Deserialize};
use alloc::string::String;

pub const DIGEST_LEN: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
pub struct Digest(pub [u8; DIGEST_LEN]);

impl Digest {
    #[inline]
    pub const fn zero() -> Self {
        Self([0; DIGEST_LEN])
    }

    #[inline]
    pub fn as_bytes(&self) -> &'_ [u8] {
        &self.0
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        *self == Self::zero()
    }
}

impl From<blake2b_simd::Hash> for Digest {
    fn from(input: blake2b_simd::Hash) -> Self {
        let data = input.as_bytes();
        debug_assert_eq!(data.len(), DIGEST_LEN);
        let mut out = Self::default();
        out.0.copy_from_slice(&data[..DIGEST_LEN]);
        out
    }
}

pub fn blake2() -> blake2b_simd::Params {
    let mut params = blake2b_simd::Params::new();
    params.hash_length(DIGEST_LEN);
    params
}


pub trait Digestible {
    fn to_digest(&self) -> Digest;
}

impl Digestible for [u8] {
    fn to_digest(&self) -> Digest {
        Digest::from(blake2().hash(self))
    }
}

impl Digestible for str {
    fn to_digest(&self) -> Digest {
        self.as_bytes().to_digest()
    }
}

impl Digestible for String {
    fn to_digest(&self) -> Digest {
        self.as_bytes().to_digest()
    }
}

macro_rules! impl_digestable_for_numeric {
    ($x: ty) => {
        impl Digestible for $x {
            fn to_digest(&self) -> Digest {
                self.to_le_bytes().to_digest()
            }
        }
    };
    ($($x: ty),*) => {$(impl_digestable_for_numeric!($x);)*}
}

impl_digestable_for_numeric!(i8, i16, i32, i64, i128);
impl_digestable_for_numeric!(u8, u16, u32, u64, u128);
impl_digestable_for_numeric!(f32, f64);

pub fn concat_digest_ref<'a>(input: impl Iterator<Item = &'a Digest>) -> Digest {
    let mut state = blake2().to_state();
    for d in input {
        state.update(d.as_bytes());
    }
    Digest::from(state.finalize())
}

pub fn concat_digest(input: impl Iterator<Item = Digest>) -> Digest {
    let mut state = blake2().to_state();
    for d in input {
        state.update(d.as_bytes());
    }
    Digest::from(state.finalize())
}