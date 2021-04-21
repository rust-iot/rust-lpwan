//! Low-Power Wide Area Network (LPWAN) Library.
//! (Intended to) provide a unified network stack for LPWAN use
//
// https://github.com/rust-iot/rust-lpwan
// Copyright 2021 Ryan Kurte

#![no_std]

use core::fmt::Debug;

use radio::{State, Busy, Transmit, Receive, Rssi, ReceiveInfo};

#[cfg(any(test, feature="std"))]
extern crate std;

#[cfg(any(test, feature="alloc"))]
extern crate alloc;

pub mod timer;

pub mod base;

pub mod error;

pub mod mac_802154;

pub mod ip6;

pub mod prelude;


/// Timestamps are 64-bit in milliseconds
pub type Ts = u64;

/// Statically sized packet buffer
pub struct RawPacket{
    data: [u8; 256],
    len: usize,
    rssi: i16,
}

/// Default constructor for raw packets
impl Default for RawPacket {
    fn default() -> Self {
        Self {
            data: [0u8; 256],
            len: 0,
            rssi: 0,
        }
    }
}

/// Fetch data from a raw packet
impl RawPacket {
    fn data(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

/// Receive information object
#[derive(Debug, Clone, PartialEq)]
pub struct RxInfo<Address=ieee802154::mac::Address> {
    /// Source address
    pub source: Address,
    /// Receive RSSI
    pub rssi: i16,
}

/// Radio interface combines base [`radio`] traits
pub trait Radio<S: radio::RadioState, I: radio::ReceiveInfo, E: Debug>: radio::State<State=S, Error=E> + radio::Busy<Error=E> + radio::Transmit<Error=E> + radio::Receive<Info=I, Error=E> + radio::Rssi<Error=E> {}

/// Automatic Radio impl for radio devices meeting the trait constraint
impl <T, S: radio::RadioState, I: ReceiveInfo, E: Debug> Radio<S, I, E> for T where 
    T: State<State=S, Error=E> + Busy<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E>,
{}


/// MAC layer interface abstraction
///
/// The MAC layer interface is tick-based and split from other layers as
/// MAC operations are often timing sensitive or critical. 
/// Packets are internally buffered with [`Mac::transmit`] and [`Mac::receive`] functions
/// to push data for transmission and poll for received data respectively.
pub trait Mac<Address=ieee802154::mac::Address> {
    type Error;

    /// Periodic tick to poll / update MAC operation
    fn tick(&mut self) -> Result<(), Self::Error>;

    /// Check if the MAC is busy
    fn busy(&mut self) -> Result<bool, Self::Error>;

    /// Setup a packet for transmission, buffered by the MAC
    fn transmit(&mut self, dest: Address, data: &[u8], ack: bool) -> Result<(), Self::Error>;

    /// Check for received packets, buffered by the MAC
    fn receive(&mut self, data: &mut[u8]) -> Result<Option<(usize, RxInfo)>, Self::Error>;
}

// Wrap log macros to support switching between defmt and standard logging
mod log {
    #[cfg(feature = "defmt")]
    pub use defmt::{trace, debug, info, warn, error};

    #[cfg(not(feature = "defmt"))]
    pub use log::{trace, debug, info, warn, error};
}
