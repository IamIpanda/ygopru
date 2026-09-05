//! Wrapper of the original C++ random algorithm.
//!
//! Rust cannot reproduce the original C++ random algorithm bit-for-bit, so to
//! stay compatible with the original replays and other artifacts, the original
//! C++ library is compiled and exposed through the `MTRandom` binding below.

use std::ffi::c_void;

/// Number of u32 seeds a duel uses.
pub const SEED_COUNT: usize = 8;

/// The seed a ygocore duel uses.
#[derive(Clone)]
pub enum DuelSeed {
    /// Use local time as seed.
    None,
    /// Use a single number as seed.
    /// 
    /// See also: [`create_duel`](super::create_duel)
    Single(u32),
    /// use 8 number as seed.
    /// 
    /// See also: [`create_duel_v2`](super::create_duel_v2)
    Complicated([u32; SEED_COUNT]),
}

unsafe extern "C" {
    fn mtrandom_create(seeds: *const u32, len: usize) -> *mut c_void;
    fn mtrandom_create_value(value: u32) -> *mut c_void;
    fn mtrandom_destroy(handle: *mut c_void);
    fn mtrandom_rand(handle: *mut c_void) -> u32;
    fn mtrandom_discard(handle: *mut c_void, z: u64);
    fn mtrandom_get_random_integer(handle: *mut c_void, l: i32, h: i32) -> i32;
    fn mtrandom_shuffle_vector(handle: *mut c_void, data: *mut u32, count: usize);
}

unsafe impl Send for MTRandom {}

/// Wrapper of a C++ MTRandom sequence by given seed.
pub struct MTRandom {
    handle: *mut c_void,
    seed_sequence: [u32; SEED_COUNT],
}


impl MTRandom {
    /// Create a random producer via given seed.
    pub fn new(seed: DuelSeed) -> Self {
        let seed_array = match seed {
            DuelSeed::None => {
                let mut seeds = [0u32; SEED_COUNT];
                for i in 0..SEED_COUNT {
                    seeds[i] = rand::random();
                }
                seeds
            }
            DuelSeed::Single(s) => [s; SEED_COUNT],
            DuelSeed::Complicated(seq) => seq,
        };
        let handle = unsafe { mtrandom_create(seed_array.as_ptr(), seed_array.len()) };
        Self { handle, seed_sequence: seed_array }
    }

    /// Return the next random u32 in the sequence.
    pub fn rand(&self) -> u32 {
        unsafe { mtrandom_rand(self.handle) }
    }

    /// Skip the next `z` random values in the sequence.
    pub fn discard(&self, z: u64) {
        unsafe { mtrandom_discard(self.handle, z) };
    }

    /// Return a random integer in the closed range `[l, h]`.
    pub fn get_random_integer(&self, l: i32, h: i32) -> i32 {
        unsafe { mtrandom_get_random_integer(self.handle, l, h) }
    }

    /// Shuffle the deck in place with the sequence, so the replay can be reproduced.
    pub fn shuffle_deck(&self, deck: &mut [u32]) {
        unsafe { mtrandom_shuffle_vector(self.handle, deck.as_mut_ptr(), deck.len()) };
    }

    /// The seed sequence this producer was created with.
    pub fn seed_sequence(&self) -> &[u32; SEED_COUNT] {
        &self.seed_sequence
    }
}

impl Drop for MTRandom {
    fn drop(&mut self) {
        unsafe { mtrandom_destroy(self.handle) };
    }
}
