

use log::{debug, info, warn};

use ieee802154::mac::{Frame};
use radio::{Transmit, Receive, State, Rssi, ReceiveInfo, IsBusy};

use crate::{timer::Timer, mac::Mac, packet::Packet};

/// Basic CSMA/CA MAC
/// Generic over a Radio (R), Timer (T) and Buffers (B)
pub struct BasicMac<R, T, B> {
    config: BasicMacConfig,

    state: BasicMacState,
    
    ack_required: bool,
    last_tick: u32,

    radio: R,
    timer: T,

    buffer: B,
}

/// Configuration for the basic MAC
pub struct BasicMacConfig {
    /// RSSI threshold for a channel to be determined to be clear
    pub channel_clear_threshold: i16,
    /// Timeout for message ACK (if required) in milliseconds
    pub ack_timeout_ms: i16,

    pub slots_per_round: u16,
    pub slot_time_ms: u16,
}

impl Default for BasicMacConfig {
    fn default() -> Self {
        Self {
            ack_timeout_ms: 5,
            channel_clear_threshold: -90,

            slots_per_round: 10,
            slot_time_ms: 10,
        }
    }
}

/// Basic MAC states
#[derive(Debug, Clone, PartialEq)]
pub enum BasicMacState {
    Idle,
    Listening,
    Receiving,
    Transmitting,
    AwaitingAck,
    Sleeping,
}

/// Basic MAC errors
#[derive(Debug, Clone, PartialEq)]
pub enum BasicMacError<E> {

    /// Transmission buffer full
    TransmitPending,

    /// Transmission failed
    TransmitFailed(Packet),

    /// Wrapper for unhandled / underlying radio errors
    Radio(E),
}

impl <E> From<E> for BasicMacError<E> {
    fn from(e: E) -> Self {
        BasicMacError::Radio(e)
    }
}

impl <R, I, E, T, B> BasicMac<R, T, B> 
where
    R: State<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E>,
    I: ReceiveInfo + Default,
    B: AsRef<[u8]> + AsMut<[u8]>,
    T: Timer,
{
    /// Create a new MAC using the provided radio
    pub fn new(radio: R, timer: T, buffer: B, config: BasicMacConfig) -> Self {
        Self{
            config,

            state: BasicMacState::Idle,
            ack_required: false,
            last_tick: timer.ticks_ms(),

            buffer,

            timer,
            radio, 
        }
    }

    fn try_transmit<'p>(&mut self, data: &[u8]) -> Result<bool, BasicMacError<E>> {
        // Check the radio is not currently busy
        let radio_state = self.radio.get_state()?;
        if radio_state.is_busy() {
            return Ok(false)
        }

        // Enter receive mode and 
        self.radio.start_receive()?;

        // Check that we can't hear anyone else using the channel
        let rssi = self.radio.poll_rssi()?;
        if rssi > self.config.channel_clear_threshold {
            // TODO: increase backoff
            return Ok(false)
        } else {
            // TODO: reset backoff
        }

        // Start the transmission
        self.radio.start_transmit(data)?;

        // Update MAC state
        self.state = BasicMacState::Transmitting;
        self.last_tick = self.timer.ticks_ms();

        Ok(true)
    }

}


impl <R, I, E, T, B> Mac for BasicMac<R, T, B> 
where
    R: Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E>,
    I: ReceiveInfo + Default,
    B: AsRef<[u8]> + AsMut<[u8]>,
    T: Timer,
{
    type Error = BasicMacError<E>;

    fn transmit(&mut self, packet: Packet) -> Result<(), Self::Error> {
        // Put a packet in the TX buffer


        unimplemented!();
    }

    fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        // Pop a packet from the RX buffer
        unimplemented!();
    }

    fn tick(&mut self) -> Result<(), Self::Error> {
        let now = self.timer.ticks_ms();

        match self.state {
            BasicMacState::Idle => {
                debug!("MAC entering receive state");

                // Initialise radio
                self.radio.start_receive()?;

                // Update mac state
                self.state = BasicMacState::Listening;

            },
            BasicMacState::Listening | BasicMacState::Receiving | BasicMacState::AwaitingAck => {
                // Check for receive complete
                if !self.radio.check_receive(true)? {
                    return Ok(())
                }

                debug!("MAC received packet at {} ms", now);

                // Fetch received packets
                let mut info = I::default();
                let n = self.radio.get_received(&mut info, self.buffer.as_mut())?;

                // Restart receive mode
            },
            BasicMacState::Transmitting => {
                // Check for transmission complete
                if !self.radio.check_transmit()? {
                    return Ok(())
                }

                debug!("MAC transmit complete, starting receive\r\n");

                self.radio.start_receive()?;

                self.state = BasicMacState::Listening;

            },
            BasicMacState::Sleeping => {

            }
        }

        Ok(())
    }
}


#[cfg(test)]
mod test {
    use radio::mock::*;
    
    use crate::timer::mock::MockTimer;
    use super::*;

    use std::vec;

    #[test]
    fn init_mac() {
        let mut radio = MockRadio::new(&[
            Transaction::start_receive(None),
        ]);
        let mut mac = BasicMac::new(radio.clone(), MockTimer(0), vec![0u8; 128], BasicMacConfig::default());

        mac.tick().unwrap();

        radio.done();
    }

    #[test]
    fn try_transmit_channel_clear() {
        let data = [0, 1, 2, 3, 4, 5];

        let mut radio = MockRadio::new(&[]);
        let mut mac = BasicMac::new(radio.clone(), MockTimer(0), vec![0u8; 128], BasicMacConfig::default());

        // Setup try_transmit expectations
        radio.expect(&[
            // Check we're not currently busy
            Transaction::get_state(Ok(MockState::Idle)),
            // Enter receive mode for RSSI checking
            Transaction::start_receive(None),
            // Check noone else is (percievable) transmitting
            Transaction::poll_rssi(Ok(-90i16)),
            // Start the transmission
            Transaction::start_transmit(data.to_vec(), None),
        ]);

        // Try to start transmission
        mac.try_transmit(&data).unwrap();

        // Check expectations and state
        radio.done();
        assert_eq!(mac.state, BasicMacState::Transmitting);

        // Cycle the MAC until the transmission is complete

        radio.expect(&[
            Transaction::check_transmit(Ok(false)),
            Transaction::check_transmit(Ok(true)),
            Transaction::start_receive(None),
        ]);

        mac.tick().unwrap();
        mac.tick().unwrap();

        radio.done();

        assert_eq!(mac.state, BasicMacState::Listening);
    }
}
