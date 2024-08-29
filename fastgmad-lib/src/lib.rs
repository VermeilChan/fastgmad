//! Fast GMAD implementation
//!
//! # Feature flags
//!
//! `binary` - Recommended if you're using fastgmad in a binary as this enables some binary-related helpers.

#![cfg_attr(not(feature = "binary"), warn(missing_docs))]
#![allow(clippy::unnecessary_literal_unwrap)]

const GMA_MAGIC: &[u8] = b"GMAD";
const GMA_VERSION: u8 = 3;

mod util;

/// FastGMAD errors
pub mod error;

/// GMA extraction
pub mod extract;

/// GMA file pattern whitelist
pub mod whitelist;
