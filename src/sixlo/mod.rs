//! 6LoWPAN/IPv6 Implementation
//
// https://github.com/rust-iot/rust-lpwan
// Copyright 2021 Ryan Kurte

use core::marker::PhantomData;

use crate::{Mac, Ts};
use crate::log::{FmtError, debug, info, error};

use ieee802154::mac::{Address as MacAddress, ShortAddress, ExtendedAddress};

#[cfg(feature = "smoltcp")]
pub mod smoltcp;

pub mod headers;
use headers::{Header, Eui64, V6Addr};

pub mod frag;
use frag::*;

use self::headers::MeshHeader;


pub const IPV6_MTU: usize = 1280;

pub const DEFAULT_FRAG_SIZE: usize = 64;


/// 6LoWPAN Implementation, provides IP compatible interface to higher-layers.
/// This includes IPv6 addressing, header compression, fragmentation, 
/// and neighbour discovery and management
pub struct SixLo<M, E, const MAX_PAYLOAD: usize> {
    cfg: SixLoConfig,

    mac: M,
    mac_addr: MacAddress,
    _mac_err: PhantomData<E>,

    //eui64: Eui64,
    //v6_addr: V6Addr,

    frag: Frag<DEFAULT_FRAG_SIZE>,
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
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SixLoError<M> {
    Mac(M),
    NoTxFragSlots,
}


impl <M, E, const MAX_PAYLOAD: usize> SixLo<M, E, MAX_PAYLOAD> 
where
    M: Mac<Error=E>,
    E: FmtError,
{
    /// Create a new 6LowPAN stack instance
    pub fn new(mac: M, addr: MacAddress, cfg: SixLoConfig) -> Self {
        let frag = Frag::new(cfg.frag.clone());

        let s = Self {
            cfg,

            mac,
            _mac_err: PhantomData,
            mac_addr: addr.clone(),

            // TODO: v6 + EUI addrs? PAN IDs?
            //v6_addr: V6Addr::from(addr.into()),
            frag,
        };

        info!("Setup sixlo with address: {:?}", s.mac_addr);

        s
    }

    /// Receive a 6LoWPAN packet, returning header and data on receipt
    fn handle_rx(&mut self, now_ms: Ts, source: MacAddress, data: &[u8]) -> Result<(), SixLoError<E>> {
        // Decode headers
        let (hdr, offset) = Header::decode(&data).unwrap();

        debug!("Received {:?} from {:?}, {} bytes", hdr, source, data.len() - offset);

        // Handle fragmentation
        // TODO: other layers before / after here?
        self.frag.receive(now_ms, source, &hdr, &data[offset..])?;

        Ok(())
    }
}

impl <M, E, const MAX_PAYLOAD: usize> SixLo<M, E, MAX_PAYLOAD> 
where
    M: Mac<Error=E>,
    E: FmtError,
{
    /// Tick to update the stack
    pub fn tick(&mut self, now_ms: u64) -> Result<(), SixLoError<E>> {
        let mut buff = [0u8; MAX_PAYLOAD];

        // Tick internal MAC
        self.mac.tick().map_err(SixLoError::Mac)?;
        let mac_busy = self.mac.busy().map_err(SixLoError::Mac)?;

        // Check for (and handle) received packets from the MAC
        if let Some((n, info)) = self.mac.receive(&mut buff).map_err(SixLoError::Mac)? {
            self.handle_rx(now_ms, info.source, &buff[..n])?;
        }

        // Poll fragmentation buffer for pending fragments
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
        let mut buff = [0u8; MAX_PAYLOAD];

        // Write IPv6 headers
        // TODO: actually set these headers
        let mut header = Header::default();

        #[cfg(nope)]
        {
            // Disabled while sorting out which headers are right / useful / required
            header.mesh = Some(MeshHeader{
                final_addr: dest,
                origin_addr: self.mac_addr,
                hops_left: 7,
            });
        }

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

    /// Receive a datagram, reassembled internally
    pub fn receive(&mut self, now_ms: Ts, buff: &mut [u8]) -> Result<Option<(usize, Header)>, SixLoError<E>> {
        if let Some((h, d)) = self.frag.pop() {
            &mut buff[..d.len()].copy_from_slice(d);

            Ok(Some((d.len(), h.clone())))
        } else {
            Ok(None)
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

