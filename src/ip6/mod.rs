
use core::marker::PhantomData;

use ieee802154::mac::{Header as MacHeader};

use crate::Mac;

#[cfg(feature = "smoltcp")]
pub mod smoltcp;

pub mod headers;
use headers::Header;

use self::headers::V6Addr;


const IPV6_MTU: usize = 1280;


/// 6LoWPAN Implementation, provides IP compatible interface to higher-layers.
/// This includes IPv6 addressing, header compression, fragmentation, 
/// and neighbour discovery and management
pub struct Idk<M, E> {
    mac: M,
    _mac_err: PhantomData<E>,

    addr: V6Addr,

    frag_tx_state: FragTxState,
    frag_rx_state: FragRxState,

    tx_buff: [u8; IPV6_MTU],
    rx_buff: [u8; IPV6_MTU],
}

// TODO: is it important to be able to receive more than one fragmented packet at once?
// seems... probable, in which case more buffers / a pooled approach might be better.

// Minimal Fragment Forwarding seems like a _better_ approach than the basic one
// https://tools.ietf.org/html/draft-ietf-6lo-minimal-fragment-01
// Maybe useful to be able to support both?


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
}

impl <M, E> Idk<M, E> 
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

    pub fn transmit(&mut self, _to: (), data: &[u8]) -> Result<(), ()> {
        if self.frag_tx_state != FragTxState::None {
            // Return busy
        }

        // Write headers

        // Fragement if required
        // TODO: configure this somewhere
        if data.len() < 80 {

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

