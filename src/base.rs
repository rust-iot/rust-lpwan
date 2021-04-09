
use core::{fmt::Debug, marker::PhantomData};


use log::{debug, info, warn};

use ieee802154::mac::*;

use crate::{Radio, timer::Timer, error::CoreError};

#[derive(Debug, Clone, PartialEq)]
pub struct Base<R, I, E> {
    radio: R,
    state: RadioState,
    _radio_err: PhantomData<E>,
    _radio_info: PhantomData<I>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RadioState {
    Idle,
    Listening,
    Receiving,
    Transmitting,
    Sleeping,
}

impl <R, I, E> Base<R, I, E> 
where
    R: Radio<I, E>,
    I: radio::ReceiveInfo + Default + Debug,
    E: Clone + Debug,
{
    pub fn new(radio: R) -> Result<Self, CoreError<E>> {
        let s = Self {
            radio,
            state: RadioState::Idle,
            _radio_err: PhantomData,
            _radio_info: PhantomData,
        };

        Ok(s)
    }

    pub fn is_busy(&self) -> bool {
        match self.state {
            RadioState::Idle | RadioState::Listening => false,
            _ => true,
        }
    }

    pub fn transmit(&self, now: u32, data: &[u8]) -> Result<(), CoreError<E>> {
        // Check we're not busy
        if self.is_busy() {
            return Err(CoreError::Busy);
        }

        info!("Transmit {} bytes at {} ms", n, data.len(), now);

        // Start the transmission
        self.radio.start_transmit(&data).map_err(CoreError::Radio)?;

        // Update MAC state
        self.state = RadioState::Transmitting;

        Ok(())
    }

    pub fn tick(&mut self) -> Result<(), CoreError<E>> {
        use RadioState::*;

        match self.state {
            Idle => (),
            Listening => {

            },
            Receiving => {

            },
            Transmitting => {

            },
            Sleeping, => {

            },
        }

        unimplemented!()
    }
}
