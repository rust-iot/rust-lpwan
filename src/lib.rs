//! Low-Power Wide Area Network (LPWAN) Library.
//! (Intended to) provide a unified network stack for LPWAN use
//
// https://github.com/rust-iot/rust-lpwan
// Copyright 2021 Ryan Kurte

#![no_std]
#![feature(const_generics_defaults)]

use core::convert::TryFrom;
use core::fmt::Debug;
use core::ops::{Deref, DerefMut};

use radio::{Busy, RadioState, Receive, ReceiveInfo, Rssi, State, Transmit};

#[cfg(any(test, feature = "std"))]
extern crate std;

#[cfg(any(test, feature = "alloc"))]
extern crate alloc;

/// Common radio control, shared between MACs
pub mod base;
/// Shared error types
pub mod error;
/// 802.15.4 MAC implementation
pub mod mac_802154;
/// 6LowPAN adaptation layer over MAC abstraction
pub mod sixlo;
/// Timer abstraction for stack use
pub mod timer;

pub mod prelude;

/// Timestamps are 64-bit in milliseconds
pub type Ts = u64;

/// Statically sized packet buffer
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RawPacket<const N: usize = 256> {
    pub data: [u8; N],
    pub len: usize,
    pub rssi: i16,
}

/// Default constructor for raw packets
impl<const N: usize> Default for RawPacket<N> {
    fn default() -> Self {
        Self {
            data: [0u8; N],
            len: 0,
            rssi: 0,
        }
    }
}

impl<const N: usize> RawPacket<N> {
    // Fetch length-bounded data
    pub fn data(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

/// Access raw packet buffer (non-bounded)
impl<const N: usize> Deref for RawPacket<N> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.data
    }
}

/// Access raw packet buffer (non-bounded)
impl<const N: usize> DerefMut for RawPacket<N> {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

impl<const N: usize> TryFrom<&[u8]> for RawPacket<N> {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        // Check length is okay
        if value.len() > N {
            return Err(());
        }

        // Copy data
        let mut data = [0u8; N];
        data[..value.len()].copy_from_slice(value);

        // Return RawPacket
        Ok(Self {
            data,
            len: value.len(),
            rssi: 0,
        })
    }
}

/// Receive information object
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RxInfo<Address = ieee802154::mac::Address> {
    /// Source address
    pub source: Address,
    /// Receive RSSI
    pub rssi: i16,
}

/// Radio interface combines base [`radio`] traits
pub trait Radio:
    radio::State<State = <Self as Radio>::State, Error = <Self as Radio>::Error>
    + radio::Busy<Error = <Self as Radio>::Error>
    + radio::Transmit<Error = <Self as Radio>::Error>
    + radio::Receive<Info = <Self as Radio>::Info, Error = <Self as Radio>::Error>
    + radio::Rssi<Error = <Self as Radio>::Error>
{
    type State: RadioState + Debug;
    type Info: ReceiveInfo + Debug + Default;
    type Error: Debug;
}

/// Automatic Radio impl for radio devices meeting the trait constraint
impl<T, E: Debug> Radio for T
where
    T: State<Error = E>
        + Busy<Error = E>
        + Transmit<Error = E>
        + Receive<Error = E>
        + Rssi<Error = E>,
    <T as State>::State: RadioState + Debug,
    <T as Receive>::Info: ReceiveInfo + Debug + Default,
{
    type Error = E;
    type State = <T as State>::State;
    type Info = <T as Receive>::Info;
}

/// Network interface abstraction
pub trait Mac<Address = ieee802154::mac::Address> {
    type Error: MacError + Debug;

    /// Fetch MAC layer state
    fn state(&self) -> Result<MacState<Address>, Self::Error>;

    /// Periodic tick to poll / update layer operation
    fn tick(&mut self) -> Result<(), Self::Error>;

    /// Check if the layer is busy, used to avoid interrupting MAC operations
    fn busy(&mut self) -> Result<bool, Self::Error>;

    /// Check for transmit buffer capacity
    fn can_transmit(&self) -> Result<bool, Self::Error>;

    /// Setup a packet for transmission, buffered by the implementer
    fn transmit(&mut self, dest: Address, data: &[u8], ack: bool) -> Result<(), Self::Error>;

    /// Check for received packets, buffered by the implementer
    fn receive(&mut self, data: &mut [u8])
        -> Result<Option<(usize, RxInfo<Address>)>, Self::Error>;
}

pub trait MacError {
    fn queue_full(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq, strum::Display)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MacState<Address = ieee802154::mac::Address> {
    Disconnected,
    Synced(Address),
    Associated(Address),
}

// Wrap log macros to support switching between defmt and standard logging

#[cfg(feature = "defmt")]
mod log {
    pub use defmt::{debug, error, info, trace, warn};

    pub trait FmtError: core::fmt::Debug + defmt::Format {}
    impl<T: core::fmt::Debug + defmt::Format> FmtError for T {}
}
#[cfg(not(feature = "defmt"))]
mod log {
    pub use log::{debug, error, info, trace, warn};

    pub trait FmtError: core::fmt::Debug {}
    impl<T: core::fmt::Debug> FmtError for T {}
}

pub trait Alloc {
    /// Container type for the given allocator
    type C: AsRef<[u8]> + AsMut<[u8]>;

    /// Request an allocation of N size
    fn req(size: usize) -> Option<Self::C>;
}

pub struct ConstAlloc<const N: usize = 256> {}

impl<const N: usize> Alloc for ConstAlloc<N> {
    type C = [u8; N];

    fn req(size: usize) -> Option<Self::C> {
        if size < N {
            Some([0u8; N])
        } else {
            None
        }
    }
}

#[cfg(feature = "alloc")]
pub struct StdAlloc {}

#[cfg(feature = "alloc")]
impl Alloc for StdAlloc {
    type C = alloc::vec::Vec<u8>;

    fn req(size: usize) -> Option<Self::C> {
        Some(alloc::vec![0u8; size])
    }
}
