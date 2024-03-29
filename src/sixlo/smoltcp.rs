//! 6LoWPAN/IPv6 smoltcp support
//
// https://github.com/rust-iot/rust-lpwan
// Copyright 2021 Ryan Kurte

use smoltcp::{phy, time::Instant};

use crate::log::info;

use super::SixLo;
use crate::Mac;

// TODO: how to implement smolctp device on top of 6lo + 802.15.4?
impl<'a, M, const MAX_PAYLOAD: usize> phy::Device<'a> for SixLo<M, MAX_PAYLOAD>
where
    M: Mac,
    <M as Mac>::Error: core::fmt::Debug,
{
    type RxToken = RxToken<'a>;
    type TxToken = TxToken<'a>;

    fn receive(&'a mut self) -> Option<(Self::RxToken, Self::TxToken)> {
        //Some((RxToken(&mut self.rx_buffer[..]), TxToken(&mut self.tx_buffer[..])))
        None
    }

    fn transmit(&'a mut self) -> Option<Self::TxToken> {
        //Some(TxToken(&mut self.tx_buffer[..]))
        None
    }

    fn capabilities(&self) -> phy::DeviceCapabilities {
        let mut caps = phy::DeviceCapabilities::default();
        // TODO: fix this
        caps.max_transmission_unit = 1536;
        caps.max_burst_size = Some(1);
        caps
    }
}

// TODO: how to interact via tokens? the MAC needs to continue ticking etc. so,
// maybe this could be buffered?
pub struct RxToken<'a>(&'a mut [u8]);

impl<'a> phy::RxToken for RxToken<'a> {
    fn consume<R, F>(mut self, _timestamp: Instant, f: F) -> smoltcp::Result<R>
    where
        F: FnOnce(&mut [u8]) -> smoltcp::Result<R>,
    {
        // TODO: receive packet into buffer
        let result = f(&mut self.0);
        info!("rx called");
        result
    }
}

pub struct TxToken<'a>(&'a mut [u8]);

impl<'a> phy::TxToken for TxToken<'a> {
    fn consume<R, F>(self, _timestamp: Instant, len: usize, f: F) -> smoltcp::Result<R>
    where
        F: FnOnce(&mut [u8]) -> smoltcp::Result<R>,
    {
        let result = f(&mut self.0[..len]);
        info!("tx called {}", len);
        // TODO: send packet out
        result
    }
}
