
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
