
#![no_std]

#[cfg(test)]
extern crate std;

pub mod timer;

pub mod packet;

pub mod mac;

#[derive(Clone, Debug, PartialEq)]
pub struct NetConfig {
    pub pan_id: u16,
    pub short_addr: u16,
    pub long_addr: u64,
}

pub trait Radio<I, E>: radio::State<Error=E> + radio::Busy<Error=E> + radio::Transmit<Error=E> + radio::Receive<Info=I, Error=E> + radio::Rssi<Error=E> {}


