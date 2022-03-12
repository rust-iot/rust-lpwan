//! Low-Power Wide Area Network (LPWAN) Library.
//! (Intended to) provide a unified network stack for LPWAN use
//
// https://github.com/rust-iot/rust-lpwan
// Copyright 2021 Ryan Kurte

#![no_std]

#![feature(const_generics_defaults)]

use core::fmt::Debug;

use radio::{State, Busy, Transmit, Receive, Rssi, ReceiveInfo, RadioState};

#[cfg(any(test, feature="std"))]
extern crate std;

#[cfg(any(test, feature="alloc"))]
extern crate alloc;

pub mod timer;

pub mod base;

pub mod error;

pub mod mac_802154;

pub mod sixlo;

pub mod prelude;


/// Timestamps are 64-bit in milliseconds
pub type Ts = u64;

/// Statically sized packet buffer
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RawPacket<const N: usize = 256>{
    data: [u8; N],
    len: usize,
    rssi: i16,
}

/// Default constructor for raw packets
impl <const N: usize> Default for RawPacket<N> {
    fn default() -> Self {
        Self {
            data: [0u8; N],
            len: 0,
            rssi: 0,
        }
    }
}

/// Fetch data from a raw packet
impl <const N: usize> RawPacket<N> {
    fn data(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

/// Receive information object
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RxInfo<Address=ieee802154::mac::Address> {
    /// Source address
    pub source: Address,
    /// Receive RSSI
    pub rssi: i16,
}

/// Radio interface combines base [`radio`] traits
pub trait Radio: 
        radio::State<State=<Self as Radio>::State, Error=<Self as Radio>::Error> + 
        radio::Busy<Error=<Self as Radio>::Error> + 
        radio::Transmit<Error=<Self as Radio>::Error> + 
        radio::Receive<Info=<Self as Radio>::Info, Error=<Self as Radio>::Error> + 
        radio::Rssi<Error=<Self as Radio>::Error> {

    type State: RadioState + Debug;
    type Info: ReceiveInfo + Debug + Default;
    type Error: Debug;
}

/// Automatic Radio impl for radio devices meeting the trait constraint
impl <T, E: Debug> Radio for T where 
    T: State<Error=E> + Busy<Error=E> + Transmit<Error=E> + Receive<Error=E> + Rssi<Error=E>,
    <T as State>::State: RadioState + Debug,
    <T as Receive>::Info: ReceiveInfo + Debug + Default,
{
    type Error = E;
    type State = <T as State>::State;
    type Info = <T as Receive>::Info;
}


/// Network interface abstraction
pub trait Mac<Address=ieee802154::mac::Address> {
    type Error: Debug;

    /// Fetch MAC layer state
    fn state(&self) -> Result<MacState<Address>, Self::Error>;

    /// Periodic tick to poll / update layer operation
    fn tick(&mut self) -> Result<(), Self::Error>;

    /// Check if the layer is busy, used for back-pressure
    fn busy(&mut self) -> Result<bool, Self::Error>;

    /// Setup a packet for transmission, buffered by the implementer
    fn transmit(&mut self, dest: Address, data: &[u8], ack: bool) -> Result<(), Self::Error>;

    /// Check for received packets, buffered by the implementer
    fn receive(&mut self, data: &mut[u8]) -> Result<Option<(usize, RxInfo<Address>)>, Self::Error>;
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MacState<Address> {
    Disconnected,
    Synced(Address),
    Associated(Address),
}

// Wrap log macros to support switching between defmt and standard logging

#[cfg(feature = "defmt")]
mod log {
    pub use defmt::{trace, debug, info, warn, error};

    pub trait FmtError: core::fmt::Debug + defmt::Format {}
    impl <T: core::fmt::Debug + defmt::Format> FmtError for T {}

}
#[cfg(not(feature = "defmt"))]
mod log {
    pub use log::{trace, debug, info, warn, error};
    
    pub trait FmtError: core::fmt::Debug {}
    impl <T: core::fmt::Debug> FmtError for T {}
}
