//! Medium Access Control (MAC) layer module.
//! Contains MAC traits and implementations.

pub mod basic;


/// Generic MAC trait, implemented by all MACs
pub trait Mac {
    type Error;

    // Queue a packet for transmission
    fn transmit(&mut self) -> Result<(), Self::Error>;

    // Start receiving
    fn receive(&mut self) -> Result<(), Self::Error>;

    // Update the MAC state
    fn tick(&mut self) -> Result<(), Self::Error>;
}


