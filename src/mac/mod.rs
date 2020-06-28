//! Medium Access Control (MAC) layer module.
//! Contains MAC traits and implementations.

pub mod basic;

use crate::{packet::Packet};

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


