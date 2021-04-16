
use core::marker::PhantomData;

pub mod headers;

#[cfg(feature = "smoltcp")]
pub mod smoltcp;

use crate::Mac;

/// 6LoWPAN Implementation, provides IP compatible interface to higher-layers.
/// This includes IPv6 addressing, header compression, fragmentation, 
/// and neighbour discovery and management
pub struct Idk<M, E> {
    mac: M,
    _mac_err: PhantomData<E>,
}

impl <M, E> Idk<M, E> 
where
    M: Mac<E>,
    E: core::fmt::Debug,
{
    pub fn new(mac: M) -> Self {
        Self {
            mac,
            _mac_err: PhantomData,
        }
    }


}

