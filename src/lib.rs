
#![no_std]

use core::fmt::Debug;

use radio::{State, Busy, Transmit, Receive, Rssi, ReceiveInfo};

#[cfg(any(test, feature="std"))]
extern crate std;

pub mod timer;

pub mod packet;

pub mod base;

pub mod mac;

pub mod error;

pub mod prelude;


/// Timestamps are 64-bit in milliseconds
pub type Ts = u64;

/// Statically sized packet buffer
pub struct RawPacket{
    data: [u8; 256],
    len: usize,
    rssi: i16,
}

impl Default for RawPacket {
    fn default() -> Self {
        Self {
            data: [0u8; 256],
            len: 0,
            rssi: 0,
        }
    }
}

impl RawPacket {
    fn data(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

/// Radio interface combines `radio` traits
pub trait Radio<S: radio::RadioState, I: radio::ReceiveInfo, E: Debug>: radio::State<State=S, Error=E> + radio::Busy<Error=E> + radio::Transmit<Error=E> + radio::Receive<Info=I, Error=E> + radio::Rssi<Error=E> {}

/// Default Radio impl for radio devices
impl <T, S: radio::RadioState, I: ReceiveInfo, E: Debug> Radio<S, I, E> for T where 
    T: State<State=S, Error=E> + Busy<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E>,
{}
