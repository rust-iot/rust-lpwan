
use core::marker::PhantomData;

use log::{debug};

use ieee802154::mac::{Header as MacHeader, Address as MacAddress};

use crate::Mac;

#[cfg(feature = "smoltcp")]
pub mod smoltcp;

pub mod headers;
use headers::{FragHeader, Header, V6Addr};

pub mod frag;
use frag::*;


const IPV6_MTU: usize = 1280;

const MAX_FRAG_SIZE: usize = 64;

/// 6LoWPAN Implementation, provides IP compatible interface to higher-layers.
/// This includes IPv6 addressing, header compression, fragmentation, 
/// and neighbour discovery and management
pub struct SixLo<M, E> {
    mac: M,
    _mac_err: PhantomData<E>,

    addr: V6Addr,
    frag_tag: u16,

    tx_buffs: [FragTxBuffer<MAX_FRAG_SIZE>; 4],
    rx_buffs: [FragRxBuffer; 4],
}

pub struct SixLoConfig {

}

pub enum SixLoError<M> {
    Mac(M),
    NoTxFragSlots,
}


impl <M, E> SixLo<M, E> 
where
    M: Mac<Error=E>,
    E: core::fmt::Debug,
{
    pub fn new<A: Into<V6Addr>>(mac: M, addr: A) -> Self {
        Self {
            mac,
            _mac_err: PhantomData,

            addr: addr.into(),
            frag_tag: 0,
            
            tx_buffs: Default::default(),
            rx_buffs: Default::default(),
        }
    }

    pub fn tick(&mut self, now_ms: u64) -> Result<(), SixLoError<E>> {
        // TODO: configure max fragment / message size
        let mut buff = [0u8; 256];

        // Check for outgoing fragments
        for i in 0..self.tx_buffs.len() {
            let b = &mut self.tx_buffs[i];

            // TODO: probably only send these periodically..?

            // Fetch the next chunk to send
            if let Some((h, o, l)) = b.next() {
                // Encode header
                let mut n = h.encode(&mut buff);
                // Encode copy datagram fragment
                &buff[n..n+l].copy_from_slice(&b.buff[o..o+l]);
                n += l;

                // Transmit fragment
                self.mac.transmit(b.dest, &buff[..n], true).map_err(SixLoError::Mac)?;
            }

            if b.state == FragTxState::Done {
                debug!("Completed TX for fragment {}", b.tag);
                b.state = FragTxState::None;
            }
        }

        // Check for incoming fragments
        for i in 0..self.rx_buffs.len() {
            // TODO: timeout
        }

        Ok(())
    }

    /// Transmit a datagram, transparently fragmenting this as required
    pub fn transmit(&mut self, dest: MacAddress, data: &[u8]) -> Result<(), SixLoError<E>> {
        // TODO: configure max fragment / message size
        let mut buff = [0u8; 256];

        // Write IPv6 headers
        let mut header = Header::default();
        // TODO: actually set these up
        let mut n = header.encode(&mut buff);

        // Fragement if required
        if n + data.len() < buff.len() {
            // Copy data into TX buffer
            &buff[n..n+data.len()].copy_from_slice(data);
            n += data.len();

            debug!("Immediate TX {} datagram bytes", data.len());

            // Transmit directly
            self.mac.transmit(dest, &buff[..n], true).map_err(SixLoError::Mac)?;

        } else {
            // Find an empty TX fragment buffer
            let slot = match self.tx_buffs.iter_mut().find(|buff| buff.state == FragTxState::None) {
                Some(s) => s,
                None => {
                    return Err(SixLoError::NoTxFragSlots);
                }
            };

            // Setup fragmentation header
            header.frag = Some(FragHeader{
                datagram_size: data.len() as u16,
                datagram_tag: self.frag_tag,
                datagram_offset: None,
            });
            self.frag_tag = self.frag_tag.wrapping_add(1);
            
            // Initialise outgoing fragment buffer
            *slot = FragTxBuffer::init(dest, header, self.frag_tag, data);
            debug!("TX fragment slot {} byte dataframe", data.len());
        }

        Ok(())
    }

    pub fn handle_rx(&mut self, _mac_header: MacHeader, payload: &[u8]) -> Result<(), ()> {
        // Decode headers
        let (h, _o) = Header::decode(&payload).unwrap();

        // Handle fragmentation
        if let Some(frag) = &h.frag {
            // 

        }

        unimplemented!()
    }

}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_frag_defrag() {



    }

}

