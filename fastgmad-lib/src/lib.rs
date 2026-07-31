//! Fast GMAD implementation
//!
//! # Feature flags
//!
//! `binary` - Recommended if you're using fastgmad in a binary as this enables some binary-related helpers.

#![cfg_attr(not(feature = "binary"), warn(missing_docs))]

const GMA_MAGIC: &[u8] = b"GMAD";
const GMA_VERSION: u8 = 3;

mod util;

pub mod error;
pub mod extract;
pub mod whitelist;