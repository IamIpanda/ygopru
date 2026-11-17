
#![doc = include_str!("../README.md")]

#![allow(dead_code)]

#[cfg(feature = "hashbrown")]
pub use hashbrown::HashMap;
#[cfg(feature = "hashbrown")]
pub use hashbrown::HashSet;
#[cfg(not(feature = "hashbrown"))]
pub use std::collections::HashMap;
#[cfg(not(feature = "hashbrown"))]
pub use std::collections::HashSet;

mod utils;
pub mod data;
#[cfg(feature = "field")]
pub mod field;
pub mod message;
pub mod constants;

pub use utils::*;
