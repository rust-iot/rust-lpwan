
use core::fmt::Debug;

use log::{debug, info, warn, error};

use ieee802154::mac::*;
use radio::{Transmit, Receive, State, Rssi, ReceiveInfo, IsBusy};

use crate::{timer::Timer, mac::Mac, packet::Packet};

/// Basic CSMA/CA MAC
/// Generic over a Radio (R), Timer (T) and Buffers (B)
pub struct BasicMac<R: Debug, T: Debug, B: Debug> {
    config: BasicMacConfig,

    state: BasicMacState,
    
    ack_required: bool,
    retries: u16,
    last_tick: u32,

    radio: R,
    timer: T,

    buffer: B,

    tx_buffer: Option<Packet>,
    rx_buffer: Option<Packet>,
}

/// Configuration for the basic MAC
#[derive(Clone, PartialEq, Debug)]
pub struct BasicMacConfig {
    /// RSSI threshold for a channel to be determined to be clear
    pub channel_clear_threshold: i16,
    
    /// Timeout for message ACK (if required) in milliseconds
    pub ack_timeout_ms: u32,

    /// Number of retries for acknowleged messages
    pub max_retries: u16,

    pub rx_has_footer: bool,
    //pub tx_write_footer: WriteFooter,

    pub send_acks: bool,

    pub slots_per_round: u16,
    pub slot_time_ms: u16,
}

impl Default for BasicMacConfig {
    fn default() -> Self {
        Self {
            
            channel_clear_threshold: -90,
            ack_timeout_ms: 10,
            max_retries: 5,

            rx_has_footer: false,
            //tx_write_footer: WriteFooter::No,

            send_acks: true,

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
    /// Buffer full
    BufferFull(Packet),

    /// Transmission buffer full
    TransmitPending,

    /// Transmission failed
    TransmitFailed(Packet),

    /// Decoding error
    DecodeError(DecodeError),

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
    R: State<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E> + Debug,
    I: ReceiveInfo + Default + Debug,
    B: AsRef<[u8]> + AsMut<[u8]> + Debug,
    T: Timer + Debug,
{
    /// Create a new MAC using the provided radio
    pub fn new(radio: R, timer: T, buffer: B, config: BasicMacConfig) -> Self {
        Self{
            config,

            state: BasicMacState::Idle,
            
            ack_required: false,
            retries: 0,

            last_tick: timer.ticks_ms(),

            buffer,

            rx_buffer: None,
            tx_buffer: None,

            timer,
            radio, 
        }
    }

    fn start_receive(&mut self) -> Result<(), BasicMacError<E>> {
        debug!("Start receive");

        // Check the radio is not currently busy
        let radio_state = self.radio.get_state()?;
        if radio_state.is_busy() {
            //TODO: what do?
        }

        // Enter receive mode
        self.radio.start_receive()?;

        // Update mac state
        self.state = BasicMacState::Listening;
        self.last_tick = self.timer.ticks_ms();

        Ok(())
    }

    fn try_transmit<'p>(&mut self, packet: &Packet) -> Result<bool, BasicMacError<E>> {
        debug!("Try transmit");

        // Check the radio is not currently busy
        let radio_state = self.radio.get_state()?;
        if radio_state.is_busy() {
            debug!("Radio busy");
            return Ok(false)
        }

        // Enter receive mode if not already
        match self.state {
            BasicMacState::Listening => (),
            _ => self.radio.start_receive()?,
        };
        
        // Check that we can't hear anyone else using the channel
        let rssi = self.radio.poll_rssi()?;
        if rssi > self.config.channel_clear_threshold {
            // TODO: increase backoff
            debug!("Channel busy");
            return Ok(false)
        }

        // TODO: reset backoff

        // Do packet transmission
        self.do_transmit(packet)?;

        Ok(true)
    }

    fn do_transmit(&mut self, packet: &Packet) -> Result<(), BasicMacError<E>> {
        debug!("Do transmit");

        let buff = self.buffer.as_mut();

        // Encode message
        let n = packet.encode(buff, WriteFooter::No);

        // Start the transmission
        self.radio.start_transmit(&buff[..n])?;

        // Update MAC state
        self.state = BasicMacState::Transmitting;

        if packet.header.ack_request {
            self.ack_required = true;
            self.retries = self.config.max_retries;
        } else {
            self.ack_required = false;
            self.retries = 0;
        }
        
        self.last_tick = self.timer.ticks_ms();

        Ok(())
    }

    fn try_receive(&mut self) -> Result<Option<Packet>, BasicMacError<E>> {
        debug!("Try receive");

        let buff = self.buffer.as_mut();
        let now = self.timer.ticks_ms();

        // Check for receive complete
        if !self.radio.check_receive(true)? {
            return Ok(None)
        }

        debug!("MAC received packet at tick {} ms", now);

        // Fetch received packets
        let mut info = I::default();
        let n = self.radio.get_received(&mut info, buff)?;

        // Decode packet
        let packet = Packet::decode(&buff[..n], self.config.rx_has_footer)
            .map_err(BasicMacError::DecodeError)?;

        // TODO: Filter packets by address
        if !self.check_address_match(&packet.header.destination) {
            return Ok(None)
        }

        Ok(Some(packet))
    }

    fn check_address_match(&self, a: &Address) -> bool {
        true
    }
}


impl <R, I, E, T, B> Mac for BasicMac<R, T, B> 
where
    R: State<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E> + Debug,
    I: ReceiveInfo + Default + Debug,
    B: AsRef<[u8]> + AsMut<[u8]> + Debug,
    T: Timer + Debug,
{
    type Error = BasicMacError<E>;

    fn transmit(&mut self, packet: Packet) -> Result<(), Self::Error> {
        // Check the buffer is not full
        if self.tx_buffer.is_some() {
            return Err(BasicMacError::BufferFull(packet))
        }

        // Put packet in buffer
        self.tx_buffer = Some(packet);

        Ok(())
    }

    fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        // Remove packet from rx buffer (if present)
        let packet = self.rx_buffer.take();

        // Return packet
        Ok(packet)
    }

    fn tick(&mut self) -> Result<Option<Packet>, Self::Error> {
        let now = self.timer.ticks_ms();

        debug!("Tick at tick {} state: {:?}", now, self.state);

        match self.state {
            BasicMacState::Idle => {
                debug!("Init, entering receive state");
                self.start_receive()?;
            },
            BasicMacState::Listening | BasicMacState::Receiving => {
                // Try to receive and unpack packet
                if let Some(packet) = self.try_receive()? {
                    debug!("Received packet");

                    // Check whether an ACK is required
                    if packet.header.ack_request {
                        
                        // Generate and transmit ack
                        let ack = Packet::ack(&packet);
                        self.do_transmit(&ack)?;

                    } else {
                        // Re-enter receive mode
                        self.start_receive()?;
                    }

                    // Put packet in rx_buffer
                    if self.rx_buffer.is_some() {
                        error!("RX buffer full, dropping received packet");
                        return Err(BasicMacError::BufferFull(packet))
                    }

                    self.rx_buffer = Some(packet);

                    return Ok(None)
                }

                // Send packet if pending
                if let Some(tx) = self.tx_buffer.take() {
                    debug!("Attempting transmission");

                    // TODO: setup ack /retry info

                    let res = self.try_transmit(&tx);
                    
                    self.tx_buffer = Some(tx);
                    let _ = res?;
                }
            },
            BasicMacState::Transmitting => {
                // Check for transmission complete
                // TODO: tx timeouts here
                if !self.radio.check_transmit()? {
                    return Ok(None)
                }

                debug!("Transmit complete");

                // Re-enter receive mode
                self.radio.start_receive()?;

                // Update state
                self.state = match self.ack_required {
                    true => BasicMacState::AwaitingAck,
                    false => BasicMacState::Listening,
                };
                self.last_tick = self.timer.ticks_ms(); 

                debug!("Transmit complete, starting receive (new state: {:?})\r\n", self.state);
            },
            BasicMacState::AwaitingAck => {
                // Receive packets if available
                let incoming = self.try_receive()?;

                // Remove outgoing packet from tx_buffer for use in the tick
                let outgoing = self.tx_buffer.take();

                match (outgoing, incoming) {
                    // On receipt of the correct ACK
                    (Some(tx), Some(rx)) if tx.is_ack(&rx) => {
                        debug!("Received ACK for packet {} at tick {}", tx.header.seq, now);
                        // Update MAC state
                        self.start_receive()?;

                        // Return completed packet
                        return Ok(Some(tx))
                    },
                    // On receipt of another packet
                    (Some(tx), Some(rx)) => {
                        debug!("Received packet mismatch (expecting ack)");
                        // TODO: store non_ack packets if we can?

                        // Replace tx in buffer
                        self.tx_buffer = Some(tx);
                    },
                    // When no packet is received
                    (Some(tx), None) if now > (self.last_tick + self.config.ack_timeout_ms) => {
                        debug!("ACK timeout for packet {} at tick {}", tx.header.seq, now);
                        
                        if self.retries > 0 {
                            // Re-attempt transmission
                            self.try_transmit(&tx)?;

                            // Replace in buffer and update retries
                            self.tx_buffer = Some(tx);
                            self.retries -= 1;
                        } else {
                            // Restart receive mode
                            self.start_receive()?;

                            // Return transmit error
                            return Err(BasicMacError::TransmitFailed(tx))
                        }
                    },
                    (Some(tx), None) => {
                        // Replace tx in buffer
                        self.tx_buffer = Some(tx);
                    }
                    (None, _) => {
                        error!("Unhandled state ({:?})", self.state);
                    },
                    _ => unreachable!(),
                }
                
            },
            BasicMacState::Sleeping => {

            }
        }

        Ok(None)
    }
}


#[cfg(test)]
mod test {
    use radio::BasicInfo;
    use radio::mock::*;
    
    use crate::timer::mock::MockTimer;
    use super::*;

    use std::vec;

    #[test]
    fn init_mac() {
        let mut radio = MockRadio::new(&[
            Transaction::get_state(Ok(MockState::Idle)),
            Transaction::start_receive(None),
        ]);
        let mut mac = BasicMac::new(radio.clone(), MockTimer::new(), vec![0u8; 128], BasicMacConfig::default());

        mac.tick().unwrap();

        radio.done();
    }

    #[test]
    fn try_transmit_channel_clear() {
        simplelog::SimpleLogger::init(log::LevelFilter::Debug, simplelog::Config::default());

        let packet = Packet::data(
            Address::Short(PanId(1), ShortAddress(2)), 
            Address::Short(PanId(1), ShortAddress(3)), 
            4, 
            &[0, 1, 2, 3, 4, 5]
        );

        let mut buff = vec![0u8; 1024];
        let n = packet.encode(&mut buff, WriteFooter::No);

        let mut radio = MockRadio::new(&[]);
        let mut timer = MockTimer::new();
        let mut mac = BasicMac::new(radio.clone(), timer.clone(), vec![0u8; 128], BasicMacConfig::default());

        // Setup try_transmit expectations
        radio.expect(&[
            // Check we're not currently busy
            Transaction::get_state(Ok(MockState::Idle)),
            // Enter receive mode for RSSI checking
            Transaction::start_receive(None),
            // Check noone else is (percievable) transmitting
            Transaction::poll_rssi(Ok(-90i16)),
            // Start the transmission
            Transaction::start_transmit((&buff[..n]).to_vec(), None),
        ]);

        timer.set_ms(1);

        // Try to start transmission
        mac.try_transmit(&packet).unwrap();

        // Check expectations and state
        radio.done();
        assert_eq!(mac.state, BasicMacState::Transmitting);
        assert_eq!(mac.last_tick, 1);
        assert_eq!(mac.ack_required, false);
        assert_eq!(mac.retries, 0);

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

    #[test]
    fn transmit_with_ack() {
        simplelog::SimpleLogger::init(log::LevelFilter::Debug, simplelog::Config::default());

        let mut packet = Packet::data(
            Address::Short(PanId(1), ShortAddress(2)), 
            Address::Short(PanId(1), ShortAddress(3)), 
            4, 
            &[0, 1, 2, 3, 4, 5]
        );

        packet.header.ack_request = true;

        let mut buff = vec![0u8; 1024];
        let n = packet.encode(&mut buff, WriteFooter::No);

        let mut radio = MockRadio::new(&[]);
        let mut timer = MockTimer::new();
        let mut mac = BasicMac::new(radio.clone(), timer.clone(), vec![0u8; 128], BasicMacConfig::default());

        // Setup try_transmit expectations
        radio.expect(&[
            Transaction::get_state(Ok(MockState::Idle)),
            Transaction::start_receive(None),
            Transaction::check_receive(true, Ok(false)),
            Transaction::get_state(Ok(MockState::Receive)),
            Transaction::poll_rssi(Ok(-90i16)),
            Transaction::start_transmit((&buff[..n]).to_vec(), None),
        ]);

        // Queue packet for transmission
        timer.set_ms(1);
        mac.transmit(packet.clone()).unwrap();

        assert_eq!(mac.tx_buffer, Some(packet.clone()));

        info!("Starting TX");

        // Tick MAC to start RX
        timer.set_ms(2);
        mac.tick().unwrap();

        // Tick MAC to start TX
        timer.set_ms(3);
        mac.tick().unwrap();

        // Check expectations and state
        assert_eq!(mac.state, BasicMacState::Transmitting);
        assert_eq!(mac.last_tick, 3);

        radio.done();


        // Setup transmit completion expectations

        radio.expect(&[
            Transaction::check_transmit(Ok(false)),
            Transaction::check_transmit(Ok(true)),
            Transaction::start_receive(None),
        ]);

        info!("Continuing TX");

        // Tick in transmitting state (still transmitting)
        timer.set_ms(4);
        mac.tick().unwrap();

        assert_eq!(mac.state, BasicMacState::Transmitting);
        assert_eq!(mac.last_tick, 3);

        info!("Completing TX");

        // Tick in transmitting state (transmission done)
        timer.set_ms(5);
        mac.tick().unwrap();

        // Check we're ready for an ACK
        assert_eq!(mac.state, BasicMacState::AwaitingAck);
        assert_eq!(mac.last_tick, 5);
        assert_eq!(mac.ack_required, true);
        assert_eq!(mac.retries, mac.config.max_retries);
        assert_eq!(mac.tx_buffer, Some(packet.clone()));

        radio.done();
    }

    #[test]
    fn ack_receive() {
        simplelog::SimpleLogger::init(log::LevelFilter::Debug, simplelog::Config::default());

        let mut packet = Packet::data(
            Address::Short(PanId(1), ShortAddress(2)), 
            Address::Short(PanId(1), ShortAddress(3)), 
            4, 
            &[0, 1, 2, 3, 4, 5]
        );
        packet.header.ack_request = true;

        let mut radio = MockRadio::new(&[]);
        let mut timer = MockTimer::new();
        let mut mac = BasicMac::new(radio.clone(), timer.clone(), vec![0u8; 128], BasicMacConfig::default());

        // Configure MAC into AwaitingAck state
        mac.state = BasicMacState::AwaitingAck;
        mac.ack_required = true;
        mac.retries = mac.config.max_retries;
        mac.tx_buffer = Some(packet.clone());

        // Receive ACK message
        let ack = Packet::ack(&packet);
        let mut buff = [0u8; 256];
        let n = ack.encode(&mut buff, WriteFooter::No);

        radio.expect(&[
            Transaction::check_receive(true, Ok(false)),
            Transaction::check_receive(true, Ok(true)),
            Transaction::get_received(Ok(((&buff[..n]).to_vec(), BasicInfo::default()))),
            Transaction::get_state(Ok(MockState::Idle)),
            Transaction::start_receive(None),
        ]);

        // First tick, no RX, no change in state
        timer.set_ms(1);
        mac.tick().unwrap();

        assert_eq!(mac.state, BasicMacState::AwaitingAck);
        assert_eq!(mac.last_tick, 0);

        // Second tick, Receive ack
        timer.set_ms(2);
        mac.tick().unwrap();

        // Return to receive mode
        assert_eq!(mac.state, BasicMacState::Listening);
        assert_eq!(mac.last_tick, 2);

        radio.done();
    }
}
