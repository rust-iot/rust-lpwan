use core::fmt::Debug;

use log::{debug, trace, info, warn, error};

use radio::{Transmit, Receive, State, Busy, Rssi, ReceiveInfo};

use rand_core::RngCore;
use rand_facade::GlobalRng;

use crate::{Radio, timer::Timer, mac::Mac, packet::Packet};

use super::config::*;
use super::error::*;
use super::core::*;


#[derive(Debug, PartialEq)]
pub struct TschMode {
    config: TschConfig,
    ctx: TschCtx,
}

#[derive(Debug, PartialEq)]
pub struct TschCtx {
    // Network join state
    state: NetworkState,

    // Outgoing packet buffer

    // Incoming packet buffer
}

#[derive(Debug, PartialEq)]
pub enum NetworkState {
    Idle,
    Joining,
}


#[derive(Debug, PartialEq)]
pub struct TschConfig {
    slot_len_us: u32,
}

impl Default for TschConfig {
    fn default() -> Self {
        Self {
            slot_len_us: 10 * 1000,
        }
    }
}

impl <R, I, E, T, B> Core<R, T, B, TschMode> 
where
    R: State<Error=E> + Busy<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E>,
    I: ReceiveInfo + Debug + Default,
    B: AsRef<[u8]> + AsMut<[u8]> + Debug,
    T: Timer,
{
    /// Create a new MAC using the provided radio
    pub fn new_tsch(radio: R, timer: T, buffer: B, address: AddressConfig, tsch_config: TschConfig, core_config: CoreConfig) -> Self {
        let mode = TschMode {
            config: tsch_config,
            ctx: TschCtx{
                state: NetworkState::Idle,
            },
        };
        Core::new_with_mode(radio, timer, buffer, address, core_config, mode)
    }
}

impl <R, I, E, T, B> Mac for Core<R, T, B, TschMode> 
where
    R: State<Error=E> + Busy<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E>,
    I: ReceiveInfo + Default + Debug,
    B: AsRef<[u8]> + AsMut<[u8]>,
    T: Timer,
{
    type Error = CoreError<E>;

    fn transmit(&mut self, packet: Packet) -> Result<(), Self::Error> {
        unimplemented!()
    }

    fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        unimplemented!()
    }

    fn tick(&mut self) -> Result<(), Self::Error> {
        
        match self.mode.ctx.state {
            NetworkState::Idle => {
                debug!("Init, entering receive state");
                self.receive_start()?;

                self.mode.ctx.state = NetworkState::Joining;
            },
            NetworkState::Joining => {
                // Try to receive and unpack packet
                match self.try_receive() {
                    Ok(Some(p)) => self.handle_received(p),
                    Ok(None) => return Ok(()),
                    Err(e) => {
                        self.receive_start()?;
                        return Err(e);
                    }
                };

                // Check RX buffer
                if let Some(p) = self.rx_buffer.take() {
                    debug!("RX: {:?}", p)

                    // TODO: handle beacons etc.

                    // TODO: place in RX buffer
                }
            }
        }

        Ok(())
    }
}

impl <R, I, E, T, B> Core<R, T, B, TschMode> 
where
    R: State<Error=E> + Busy<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E>,
    I: ReceiveInfo + Debug + Default,
    B: AsRef<[u8]> + AsMut<[u8]> + Debug,
    T: Timer,
{
    fn run_idle(&self) -> Result<(), CoreError<E>> {
        unimplemented!()
    }

}