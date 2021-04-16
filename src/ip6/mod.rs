
use core::marker::PhantomData;

use ieee802154::mac::{Header as MacHeader};

use crate::Mac;

#[cfg(feature = "smoltcp")]
pub mod smoltcp;

pub mod headers;
use headers::Header;

use self::headers::V6Addr;


const IPV6_MTU: usize = 1280;

const MAX_FRAG_SIZE: usize = 64;

/// 6LoWPAN Implementation, provides IP compatible interface to higher-layers.
/// This includes IPv6 addressing, header compression, fragmentation, 
/// and neighbour discovery and management
pub struct SixLo<M, E> {
    mac: M,
    _mac_err: PhantomData<E>,

    addr: V6Addr,

    frag_tx_state: FragTxState,
    frag_rx_state: FragRxState,

    tx_buff: [u8; IPV6_MTU],
    rx_buff: [u8; IPV6_MTU],
}

pub struct SixLoConfig {

}

// TODO: is it important to be able to receive more than one fragmented packet at once?
// seems... probable, in which case more buffers / a pooled approach might be better.

// Maybe useful to be able to support Minimal Fragment Forwarding / other improved approaches?
// https://tools.ietf.org/html/draft-ietf-6lo-minimal-fragment-01


#[derive(Clone, PartialEq, Debug)]
pub enum FragRxState {
    None,
    Pending{
        // Headers included in the first fragment so must be stored
        header: Header,

        tag: u16,
        size: u16,
        mask: u32,

        timeout: u32,
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum FragTxState {
    None,
    Sending {
        header: Header,

        tag: u16,
        size: u16,

        index: usize,

        timeout: u32,
    }
}

impl <M, E> SixLo<M, E> 
where
    M: Mac<E>,
    E: core::fmt::Debug,
{
    pub fn new<A: Into<V6Addr>>(mac: M, addr: A) -> Self {
        Self {
            mac,
            _mac_err: PhantomData,

            addr: addr.into(),
            
            frag_tx_state: FragTxState::None,
            frag_rx_state: FragRxState::None,

            tx_buff: [0u8; IPV6_MTU],
            rx_buff: [0u8; IPV6_MTU],
        }
    }

    pub fn tick(&mut self) -> Result<(), ()> {
        unimplemented!()
    }

    pub fn transmit(&mut self, _to: (), data: &[u8]) -> Result<(), ()> {
        if self.frag_tx_state != FragTxState::None {
            // Return busy
        }

        // Write headers

        // Fragement if required
        // TODO: configure this somewhere
        if data.len() < MAX_FRAG_SIZE {

        } else {

        }

        unimplemented!()
    }

    pub fn handle_rx(&mut self, _mac_header: MacHeader, payload: &[u8]) -> Result<(), ()> {
        // Decode headers
        let (h, _o) = Header::decode(&payload).unwrap();

        // Handle fragmentation
        if let Some(frag) = &h.frag {

        }

        unimplemented!()
    }

}

