
use core::{marker::PhantomData, task::Poll};

use crate::log::{debug, warn, error};

use ieee802154::mac::{Address as MacAddress, Header as MacHeader, ShortAddress, ExtendedAddress};

use crate::{Mac, Ts};

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
    cfg: SixLoConfig,

    mac: M,
    _mac_err: PhantomData<E>,

    addr: V6Addr,
    frag: Frag<MAX_FRAG_SIZE>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SixLoConfig {
    pub frag: FragConfig,
}

impl Default for SixLoConfig {
    fn default() -> Self {
        Self{
            frag: Default::default(),
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum SixLoError<M> {
    Mac(M),
    NoTxFragSlots,
}


impl <M, E> SixLo<M, E> 
where
    M: Mac<Error=E>,
    E: core::fmt::Debug,
{
    /// Create a new 6LowPAN Stack
    pub fn new<A: Into<V6Addr>>(mac: M, addr: A, cfg: SixLoConfig) -> Self {
        let frag = Frag::new(cfg.frag.clone());

        Self {
            cfg,

            mac,
            _mac_err: PhantomData,

            addr: addr.into(),
            frag,
        }
    }

    /// Tick to update the stack
    pub fn tick(&mut self, now_ms: u64) -> Result<(), SixLoError<E>> {
        // TODO: configure max fragment / message size
        let mut buff = [0u8; 256];

        // Tick internal MAC
        self.mac.tick().map_err(SixLoError::Mac)?;
        let mac_busy = self.mac.busy().map_err(SixLoError::Mac)?;

        // Check for (and handle) received packets from the MAC
        if let Some((n, info)) = self.mac.receive(&mut buff).map_err(SixLoError::Mac)? {
            self.receive(now_ms, info.source, &buff[..n])?;
        }

        // Update fragmentation layer
        let opts = PollOptions {
            can_tx: !mac_busy,
            ..Default::default()
        };
        if let Some((a, h, d)) = self.frag.poll(now_ms, opts) {
            let ack = match a {
                MacAddress::Short(_, s) if s != ShortAddress::BROADCAST => true,
                MacAddress::Extended(_, s) if s != ExtendedAddress::BROADCAST => true,
                _ => false,
            };

            // Encode header + data
            let mut n = h.encode(&mut buff);
            &buff[n..n+d.len()].copy_from_slice(d);
            n += d.len();

            // Transmit fragment
            self.mac.transmit(a, &buff[..n], ack).map_err(SixLoError::Mac)?;
        }

        Ok(())
    }

    /// Transmit a datagram, fragmenting this as required
    pub fn transmit(&mut self, now_ms: Ts, dest: MacAddress, data: &[u8]) -> Result<(), SixLoError<E>> {
        // TODO: configure max fragment / message size
        let mut buff = [0u8; 127];

        // Write IPv6 headers
        // TODO: actually set these headers
        let mut header = Header::default();
        let mut n = header.encode(&mut buff);

        let ack = match dest {
            MacAddress::Short(_, s) if s != ShortAddress::BROADCAST => true,
            MacAddress::Extended(_, s) if s != ExtendedAddress::BROADCAST => true,
            _ => false,
        };

        // If we don't need to fragment, send directly
        if n + data.len() < buff.len() {
            // Copy data into TX buffer
            &buff[n..n+data.len()].copy_from_slice(data);
            n += data.len();

            debug!("Immediate TX {} byte datagram", data.len());

            // Transmit directly
            self.mac.transmit(dest, &buff[..n], ack).map_err(SixLoError::Mac)?;

        // Otherwise, add the datagram to the fragmentation buffer
        } else {
            debug!("Fragmented TX {} byte datagram", data.len());

            if let Err(e) = self.frag.transmit(now_ms, dest, header, data) {
                error!("Failed to add datagram to fragmentation buffer: {:?}", e);
                return Err(e);
            }
        }

        Ok(())
    }

    /// Receive a 6LoWPAN packet, returning header and data on receipt
    fn receive<'a>(&'a mut self, now_ms: Ts, source: MacAddress, data: &'a[u8]) -> Result<Option<(Header, &'a [u8])>, SixLoError<E>> {
        // Decode headers
        let (hdr, offset) = Header::decode(&data).unwrap();

        // Handle fragmentation
        if let Some(frag) = &hdr.frag {
            if let Some((h, d)) = self.frag.receive(now_ms, source, &hdr, &data[offset..])? {
                debug!("Received {:?} from {:?}, {} bytes", h, source, d.len());
                Ok(Some((h.clone(), d)))
            } else {
                Ok(None)
            }
        } else {
            debug!("Received {:?} from {:?}, {} bytes", hdr, source, data.len() - offset);
            Ok(Some((hdr, &data[offset..])))
        }
    }

}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_frag_defrag() {



    }

}

