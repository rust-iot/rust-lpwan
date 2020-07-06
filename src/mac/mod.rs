//! Medium Access Control (MAC) layer module.
//! Contains MAC traits and implementations.

pub mod config;
pub use config::*;

pub mod error;
pub use error::*;

pub mod csma;
use csma::CsmaMode;

pub mod core;

use crate::{packet::Packet};

pub use ieee802154::mac::*;

/// Type alias for CSMA based MAC
pub type CsmaMac<R, T, B> = core::Core<R, T, B, CsmaMode>;


/// Generic MAC trait, implemented by all MACs
pub trait Mac {
    type Error;

    // Queue a packet for transmission
    fn transmit(&mut self, packet: Packet) -> Result<(), Self::Error>;

    // Start receiving
    fn receive(&mut self) -> Result<Option<Packet>, Self::Error>;

    // Update the MAC state
    fn tick(&mut self) -> Result<Option<Packet>, Self::Error>;
}


