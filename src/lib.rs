#[macro_use]
extern crate arrayref;
extern crate arrayvec;
extern crate blake2_c;
extern crate byteorder;
extern crate crossbeam;
#[macro_use]
extern crate lazy_static;
extern crate num_cpus;
extern crate rayon;
extern crate ring;

#[cfg(test)]
#[macro_use]
extern crate duct;
#[cfg(test)]
extern crate hex;

use byteorder::{ByteOrder, LittleEndian};
use ring::constant_time;

mod unverified;
pub mod decoder;
pub mod encoder;
pub mod hash;
pub mod io;
pub mod simple;

pub type Digest = [u8; DIGEST_SIZE];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    HashMismatch,
    ShortInput,
    Overflow,
}

pub type Result<T> = std::result::Result<T, Error>;

pub const CHUNK_SIZE: usize = 4096;
pub const DIGEST_SIZE: usize = 32;
pub const NODE_SIZE: usize = 2 * DIGEST_SIZE;
pub const HEADER_SIZE: usize = 8;

fn suffix_root(state: &mut blake2_c::blake2b::State, len: u64) {
    let mut len_bytes = [0; 8];
    LittleEndian::write_u64(&mut len_bytes, len);
    state.update(&len_bytes);
    state.set_last_node(true);
}

fn finalize_node(state: &mut blake2_c::blake2b::State) -> Digest {
    let blake_digest = state.finalize().bytes;
    *array_ref!(blake_digest, 0, ::DIGEST_SIZE)
}

fn finalize_root(state: &mut blake2_c::blake2b::State, len: u64) -> Digest {
    suffix_root(state, len);
    finalize_node(state)
}

pub fn hash_root(node: &[u8], len: u64) -> Digest {
    let mut state = blake2_c::blake2b::State::new(DIGEST_SIZE);
    state.update(node);
    finalize_root(&mut state, len)
}

// Currently we use blake2b-256, though this will get parametrized.
pub fn hash(input: &[u8]) -> Digest {
    let digest = blake2_c::blake2b_256(input);
    let mut array = [0; DIGEST_SIZE];
    array[..].copy_from_slice(&digest.bytes);
    array
}

pub fn hash_two(input1: &[u8], input2: &[u8]) -> Digest {
    let mut state = blake2_c::blake2b::State::new(DIGEST_SIZE);
    state.update(input1);
    state.update(input2);
    let digest = state.finalize();
    let mut array = [0; DIGEST_SIZE];
    array[..].copy_from_slice(&digest.bytes);
    array
}

fn hash_node(node: &[u8], suffix: &[u8]) -> Digest {
    let mut state = blake2_c::blake2b::State::new(DIGEST_SIZE);
    state.update(node);
    if !suffix.is_empty() {
        state.update(suffix);
        state.set_last_node(true);
    }
    let finalized = state.finalize();
    let mut digest = [0; DIGEST_SIZE];
    digest.copy_from_slice(&finalized.bytes);
    digest
}

fn verify_node<'a>(
    input: &'a [u8],
    len: usize,
    digest: &Digest,
    suffix: &[u8],
) -> Result<&'a [u8]> {
    if input.len() < len {
        return Err(::Error::ShortInput);
    }
    let bytes = &input[..len];
    let computed = hash_node(bytes, suffix);
    if constant_time::verify_slices_are_equal(digest, &computed).is_ok() {
        Ok(bytes)
    } else {
        Err(Error::HashMismatch)
    }
}

fn verify(input: &[u8], digest: &Digest) -> Result<()> {
    let computed = hash(input);
    constant_time::verify_slices_are_equal(&digest[..], &computed[..])
        .map_err(|_| Error::HashMismatch)
}

// Interesting input lengths to run tests on.
#[cfg(test)]
const TEST_CASES: &[usize] = &[
    0,
    1,
    10,
    CHUNK_SIZE - 1,
    CHUNK_SIZE,
    CHUNK_SIZE + 1,
    2 * CHUNK_SIZE - 1,
    2 * CHUNK_SIZE,
    2 * CHUNK_SIZE + 1,
    3 * CHUNK_SIZE - 1,
    3 * CHUNK_SIZE,
    3 * CHUNK_SIZE + 1,
    4 * CHUNK_SIZE - 1,
    4 * CHUNK_SIZE,
    4 * CHUNK_SIZE + 1,
    16 * CHUNK_SIZE - 1,
    16 * CHUNK_SIZE,
    16 * CHUNK_SIZE + 1,
];

#[cfg(test)]
mod test {
    use ::*;

    #[test]
    fn test_hash_works_at_all() {
        let inputs: &[&[u8]] = &[b"", b"f", b"foo"];
        for input in inputs {
            let mut digest = hash(input);
            verify(input, &digest).unwrap();
            digest[0] ^= 1;
            verify(input, &digest).unwrap_err();
        }
    }

    #[test]
    fn test_hash_two() {
        assert_eq!(hash(b"foobar"), hash_two(b"foo", b"bar"));
    }
}
