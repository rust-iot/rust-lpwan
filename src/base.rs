
use core::{fmt::Debug, marker::PhantomData};

use log::{trace, debug};

use crate::{Radio, RawPacket, error::CoreError};

#[derive(Debug, Clone, PartialEq)]
pub struct Base<R, S, I, E> {
    radio: R,
    state: BaseState,
    _radio_state: PhantomData<S>,
    _radio_err: PhantomData<E>,
    _radio_info: PhantomData<I>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum BaseState {
    Idle,
    Listening,
    Receiving,
    Transmitting,
    Sleeping,
}

impl <R, S, I, E> Base<R, S, I, E> 
where
    R: Radio<S, I, E>,
    S: radio::RadioState,
    I: radio::ReceiveInfo + Default + Debug,
    E: Debug,
{
    /// Create a new MAC base
    pub fn new(radio: R) -> Result<Self, CoreError<E>> {
        let s = Self {
            radio,
            state: BaseState::Idle,
            _radio_state: PhantomData,
            _radio_err: PhantomData,
            _radio_info: PhantomData,
        };

        Ok(s)
    }

    /// Fetch the MAC radio state
    pub fn state(&self) -> BaseState {
        self.state
    }

    /// Check if the MAC radio is busy
    pub fn is_busy(&self) -> bool {
        use BaseState::*;

        match self.state {
            Idle | Sleeping | Listening => false,
            _ => true,
        }
    }

    pub fn sleep(&mut self) -> Result<(), CoreError<E>> {
        // Check we're not busy
        if self.is_busy() {
            return Err(CoreError::Busy);
        }

        self.radio.set_state(S::sleep()).map_err(CoreError::Radio)?;
        self.state = BaseState::Sleeping;

        Ok(())
    }

    /// Transmit a packet (immediately), this will fail if the radio is busy
    pub fn transmit(&mut self, now: u64, data: &[u8]) -> Result<(), CoreError<E>> {
        // Check we're not busy
        if self.is_busy() {
            return Err(CoreError::Busy);
        }

        debug!("Transmit {} bytes at {} ms", data.len(), now);
        trace!("{:02x?}", data);

        // Start the transmission
        self.radio.start_transmit(&data).map_err(CoreError::Radio)?;

        // Update MAC state
        self.state = BaseState::Transmitting;

        Ok(())
    }

    /// Set the MAC radio up for packet receipt, this will fail if the radio is busy
    pub fn receive(&mut self, now: u64) -> Result<(), CoreError<E>> {
        // Check we're not busy
        if self.is_busy() {
            return Err(CoreError::Busy);
        }

        debug!("Start receive at {} ms", now);
        self.radio.start_receive().map_err(CoreError::Radio)?;
        self.state = BaseState::Listening;

        Ok(())
    }

    /// Fetch the channel RSSI
    pub fn rssi(&mut self, _now: u64) -> Result<i16, CoreError<E>> {
        // Check we're not busy
        if self.is_busy() {
            return Err(CoreError::Busy);
        }

        // Read the RSSI
        let rssi = self.radio.poll_rssi().map_err(CoreError::Radio)?;

        Ok(rssi)
    }

    /// Tick to update the MAC radio device
    pub fn tick(&mut self, now: u64) -> Result<Option<RawPacket>, CoreError<E>> {
        use BaseState::*;

        match self.state {
            Idle => {
                // TODO: Auto-start here or not?
            },
            Listening | Receiving => {
                // Check for received completion and return to caller
                if let Some(rx) = self.check_receive(now)? {
                    return Ok(Some(rx));
                }
                // TODO: periodic check we're okay in the RX state?
            },
            Transmitting => {
                // Check for transmit completion
                self.check_transmit(now)?;
            },
            Sleeping => {
                // TODO: pre-emptive wake here on sleep timeout?
            },
        }

        Ok(None)
    }

    /// Internal function for receive state(s)
    fn check_receive(&mut self, now: u64) -> Result<Option<RawPacket>, CoreError<E>> {
        // TODO: Check if we're currently receiving a packet and update state

        // Check for any received packets (and re-enter RX if required)
        if !self.radio.check_receive(true).map_err(CoreError::Radio)? {
            return Ok(None)
        }

        let mut pkt = RawPacket::default();
        let mut info = I::default();

        // Fetch received packet
        pkt.len = self.radio.get_received(&mut info, &mut pkt.data).map_err(CoreError::Radio)?;
        pkt.rssi = info.rssi();

        debug!("Received {} bytes with RSSI {} at {} ms", pkt.len, info.rssi(), now);
        trace!("{:02x?}", pkt.data());

        // Restart RX
        self.radio.start_receive().map_err(CoreError::Radio)?;
        self.state = BaseState::Listening;

        Ok(Some(pkt))
    }

    /// Internal function for transmit state(s)
    fn check_transmit(&mut self, now: u64) -> Result<(), CoreError<E>> {
        // Check for tx completion
        if !self.radio.check_transmit().map_err(CoreError::Radio)? {
            return Ok(());
        }

        debug!("Transmit complete at {} ms", now);

        // Re-enter receive mode and update state
        self.radio.start_receive().map_err(CoreError::Radio)?;
        self.state = BaseState::Listening;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use radio::{BasicInfo, mock::*};

    #[test]
    fn init() {

        let mut radio = MockRadio::new(&[]);

        radio.expect(&[
            Transaction::start_receive(None),
        ]);
        let _base = Base::new(radio).unwrap();
    }

    #[test]
    fn receive() {
        let mut ts = 0;
        let mut radio = MockRadio::new(&[]);

        let mut base = Base::new(radio.clone()).unwrap();
        assert_eq!(base.state(), BaseState::Idle);

        // Start receive mode
        radio.expect(&[
            Transaction::start_receive(None),
        ]);
        base.receive(ts).unwrap();
        ts += 1;

        // No RX yet
        radio.expect(&[
            Transaction::check_receive(true, Ok(false)),
        ]);
        base.tick(ts).unwrap();
        assert_eq!(base.state(), BaseState::Listening);
        ts += 1;

        // RX packet
        radio.expect(&[
            Transaction::check_receive(true, Ok(true)),
            Transaction::get_received(Ok((std::vec![00, 11, 22, 33], BasicInfo::default()))),
            Transaction::start_receive(None),
        ]);
        let rx = base.tick(ts).unwrap();

        // Return to listening state
        assert_eq!(base.state(), BaseState::Listening);
        assert_eq!(rx.is_some(), true);

        radio.done();
    }

    #[test]
    fn transmit() {
        let mut ts = 0;
        let mut radio = MockRadio::new(&[]);

        let mut base = Base::new(radio.clone()).unwrap();
        assert_eq!(base.state(), BaseState::Idle);

        // Start receive mode
        radio.expect(&[
            Transaction::start_transmit(std::vec![00, 11, 22], None),
        ]);
        base.transmit(ts, &[00, 11, 22]).unwrap();
        ts += 1;

        // TX not yet complete
        radio.expect(&[
            Transaction::check_transmit(Ok(false)),
        ]);
        base.tick(ts).unwrap();
        assert_eq!(base.state(), BaseState::Transmitting);
        ts += 1;

        // RX packet
        radio.expect(&[
            Transaction::check_transmit(Ok(true)),
            Transaction::start_receive(None),
        ]);
        let _rx = base.tick(ts).unwrap();

        // Return to listening state
        assert_eq!(base.state(), BaseState::Listening);

        radio.done();
    }

}
