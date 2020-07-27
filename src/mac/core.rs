
use core::fmt::Debug;

use log::{trace, debug, warn, error};

use ieee802154::mac::*;

use super::config::*;
use super::error::*;

use radio::{Transmit, Receive, State, Busy, Rssi, ReceiveInfo};
use crate::{timer::Timer, packet::Packet};

/// Core MAC states
#[derive(Debug, Clone, PartialEq)]
pub enum CoreState {
    Idle,
    Listening,
    Receiving,
    Transmitting,
    AwaitingAck,
    Sleeping,
}

/// Basic CSMA/CA MAC
/// Generic over a Radio (R), Timer (T), Buffers (B) and Mode (M)
pub struct Core<R, T, B, M> {
    pub(crate) address: AddressConfig,
    pub(crate) config: CoreConfig,

    pub(crate) state: CoreState,
    pub(crate) seq: u8,
    
    pub(crate) ack_required: bool,
    pub(crate) retries: u16,
    pub(crate) last_tick: u32,

    pub(crate) radio: R,
    pub(crate) timer: T,
    pub(crate) mode: M,

    /// Buffer for encode/decode operations
    pub(crate) buffer: B,

    /// TX buffer for outgoing packets
    pub(crate) tx_buffer: Option<Packet>,
    /// RX buffer for incoming packets
    pub(crate) rx_buffer: Option<Packet>,
}

impl <R, I, E, T, B> Core<R, T, B, ()> 
where
    R: State<Error=E> + Busy<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E>,
    I: ReceiveInfo + Default + Debug,
    B: AsRef<[u8]> + AsMut<[u8]>,
    T: Timer,
{
    /// Create a new MAC using the provided radio
    pub fn new(radio: R, timer: T, buffer: B, address: AddressConfig, core_config: CoreConfig) -> Self {
        Self{
            address,
            config: core_config,

            state: CoreState::Idle,
            seq: 0,
            
            ack_required: false,
            retries: 0,

            last_tick: timer.ticks_ms(),

            buffer,

            rx_buffer: None,
            tx_buffer: None,

            timer,
            radio,
            mode: (),
        }
    }
}

impl <R, I, E, T, B, M> Core<R, T, B, M> 
where
    R: State<Error=E> + Busy<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E>,
    I: ReceiveInfo + Default + Debug,
    B: AsRef<[u8]> + AsMut<[u8]>,
    T: Timer,
    M: Debug,
{
    pub fn set_transmit(&mut self, packet: Packet) -> Result<(), CoreError<E>> {
        // Check the buffer is not full
        if self.tx_buffer.is_some() {
            return Err(CoreError::BufferFull(packet))
        }

        // Put packet in buffer
        self.tx_buffer = Some(packet);

        Ok(())
    }

    pub fn get_received(&mut self) -> Result<Option<Packet>, CoreError<E>> {
        // Remove packet from rx buffer (if present)
        let packet = self.rx_buffer.take();

        // Return packet
        Ok(packet)
    }

    /// Enter receive mode
    pub fn receive_start(&mut self) -> Result<(), CoreError<E>> {
        trace!("Start receive");

        // Check the radio is not currently busy
        if self.radio.is_busy().map_err(CoreError::Radio)? {
            //TODO: what do?
        }

        // Enter receive mode
        self.radio.start_receive().map_err(CoreError::Radio)?;

        // Update mac state
        self.state = CoreState::Listening;
        self.last_tick = self.timer.ticks_ms();

        Ok(())
    }


    /// Poll radio for a received packet
    pub fn try_receive(&mut self) -> Result<Option<Packet>, CoreError<E>> {
        trace!("Try receive");

        let buff = self.buffer.as_mut();
        let now = self.timer.ticks_ms();

        // Check for receive complete
        if !self.radio.check_receive(true).map_err(CoreError::Radio)? {
            return Ok(None)
        }

        trace!("MAC received packet at tick {} ms", now);

        // Fetch received packets
        let mut info = I::default();
        let n = self.radio.get_received(&mut info, buff).map_err(CoreError::Radio)?;

        debug!("Received ({} bytes): {:?}", n, &buff[..n]);

        // Decode packet
        let packet = Packet::decode(&buff[..n], self.config.rx_has_footer)
            .map_err(CoreError::DecodeError)?;

        // TODO: Filter packets by address
        if !self.check_address_match(&packet.header.destination) {
            return Ok(None)
        }

        Ok(Some(packet))
    }

    pub fn handle_received(&mut self, packet: Packet) -> Result<(), CoreError<E>> {
        // Check whether an ACK is required
        if packet.header.ack_request {
            // Generate and transmit ack
            let ack = Packet::ack(&packet);
            self.transmit_now(&ack)?;

        } else {
            // Re-enter receive mode
            self.receive_start()?;
        }

        // Check RX buffer is not full
        if self.rx_buffer.is_some() {
            error!("RX buffer full, dropping received packet");
            return Err(CoreError::BufferFull(packet))
        }

        // Put packet in rx_buffer
        self.rx_buffer = Some(packet);

        Ok(())
    }

    pub fn handle_ack(&mut self, incoming: Packet) -> Result<bool, CoreError<E>> {
        let now = self.timer.ticks_ms();
        
        // Remove outgoing packet from tx_buffer for use in the tick
        let outgoing = match self.tx_buffer.as_ref() {
            Some(v) => v,
            None => return Ok(false),
        };

        match incoming.is_ack_for(&outgoing) {
            // On receipt of the correct ACK
            true => {
                debug!("Received ACK for packet {} at tick {}", outgoing.header.seq, now);
                // Update MAC state
                self.receive_start()?;

                // Indicate transmission success
                Ok(true)
            },
            // On receipt of another packet
            false => {
                warn!("Received packet mismatch (expecting ack)");
                // TODO: store non_ack packets if we can?

                Ok(false)
            },
        }
    }

    pub fn channel_clear(&mut self) -> Result<bool, CoreError<E>> {
        // Check the radio is not currently busy
        if self.radio.is_busy().map_err(CoreError::Radio)? {
            warn!("Radio busy");
            return Ok(false)
        }

        // Enter receive mode if not already
        match self.state {
            CoreState::Listening => (),
            _ => self.radio.start_receive().map_err(CoreError::Radio)?,
        };
        
        // Check that we can't hear anyone else using the channel
        let rssi = self.radio.poll_rssi().map_err(CoreError::Radio)?;
        if rssi > self.config.channel_clear_threshold {
            // TODO: increase backoff
            debug!("Channel busy");

            self.state = CoreState::Listening;
            return Ok(false)
        }

        Ok(true)
    }

    /// Attempt transmission (using CSMA guards)
    pub fn transmit_csma<'p>(&mut self, packet: &Packet) -> Result<bool, CoreError<E>> {
        trace!("Try transmit");

        
        // Check channel is clear
        if !self.channel_clear()? {
            return Ok(false);
        }

        // Do packet transmission
        self.transmit_now(packet)?;

        Ok(true)
    }

    /// Transmit a packet immediately (bypassing CSMA)
    pub fn transmit_now(&mut self, packet: &Packet) -> Result<(), CoreError<E>> {
        trace!("Do transmit");

        let buff = self.buffer.as_mut();

        // Encode message
        let n = packet.encode(buff, WriteFooter::No);

        debug!("Transmitting ({} bytes): {:?}", n, &buff[..n]);

        // Start the transmission
        self.radio.start_transmit(&buff[..n]).map_err(CoreError::Radio)?;

        // Update MAC state
        self.state = CoreState::Transmitting;

        if packet.header.ack_request {
            self.ack_required = true;
        } else {
            self.ack_required = false;
        }
        
        self.last_tick = self.timer.ticks_ms();

        Ok(())
    }

    /// Poll for transmit completion
    pub fn transmit_done(&mut self) -> Result<bool, CoreError<E>> {
        let now = self.timer.ticks_ms();
        
        // TODO: Check for TX timeout
        #[cfg(nope)]
        if (self.last_tick + self.config.tx_timeout_ms) > now {
            error!("TX timeout at {} ms", now);
            self.state = CoreState::Idle;
            return Err(CoreError::Timeout)
        }
        
        // Check for transmission complete
        if !self.radio.check_transmit().map_err(CoreError::Radio)? {
            return Ok(false);
        }

        trace!("Transmit complete");

        // Re-enter receive mode
        self.radio.start_receive().map_err(CoreError::Radio)?;

        // Update state
        self.state = match self.ack_required {
            true => CoreState::AwaitingAck,
            false => CoreState::Listening,
        };
        self.last_tick = now; 

        debug!("Transmit complete, starting receive (new state: {:?})\r\n", self.state);

        Ok(true)
    }

    /// Check whether an incoming address matches
    pub fn check_address_match(&self, a: &Address) -> bool {
        // Check PAN IDs
        if let Some(p) = a.pan_id() {
            if (p != PanId::broadcast()) && (p != self.address.pan_id) {
                debug!("PAN ID mismatch");
                return false;
            }
        }

        // Match on addresses
        match (a, &self.address.extended_address, &self.address.short_address) {
            (Address::None, _extended, _short) => {
                // TODO: what do?
            },
            (Address::Short(_p, s), _extended, Some(short)) => {
                if (s != short) && (s != &ShortAddress::broadcast()) {
                    debug!("Short address mismatch");
                    return false;
                }
            },
            (Address::Extended(_p, e), Some(extended), _short) => {
                if (e != extended) && (e != &ExtendedAddress::broadcast()) {
                    debug!("Extended address mismatch");
                    return false;
                }
            },
            _ => (),
        }
        
        true
    }

    pub fn transmit_data(&mut self, dest: Address, data: &[u8]) -> Result<(), CoreError<E>> {
        let p = Packet::data(dest, self.address.get(), self.seq, data);
        self.seq += 1;
        self.set_transmit(p)
    }
}



#[cfg(test)]
mod test {
    use std::vec;

    use ieee802154::mac::*;

    use radio::mock::*;
    
    use crate::timer::mock::MockTimer;
    use super::*;


    #[test]
    fn core_init_mac() {
        let mut radio = MockRadio::new(&[]);
        let mut mac: Core<_, _, _, ()> = Core::new(radio.clone(), MockTimer::new(), vec![0u8; 128], AddressConfig::new(1, 2), CoreConfig::default());

        radio.done();
    }

    #[test]
    fn core_transmit_channel_clear() {
        let _ = simplelog::SimpleLogger::init(log::LevelFilter::Debug, simplelog::Config::default());

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
        let mut mac: Core<_, _, _, ()> = Core::new(radio.clone(), timer.clone(), vec![0u8; 128], AddressConfig::new(1, 2), CoreConfig::default());

        // Setup transmit_csma expectations
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
        mac.transmit_csma(&packet).unwrap();

        // Check expectations and state
        radio.done();
        assert_eq!(mac.state, CoreState::Transmitting);
        assert_eq!(mac.last_tick, 1);
        assert_eq!(mac.ack_required, false);
        assert_eq!(mac.retries, 0);

        // Try to complete transmission

        radio.expect(&[
            Transaction::check_transmit(Ok(false)),
            Transaction::check_transmit(Ok(true)),
            Transaction::start_receive(None),
        ]);

        assert_eq!(Ok(false), mac.transmit_done());
        assert_eq!(Ok(true), mac.transmit_done());

        radio.done();

        assert_eq!(mac.state, CoreState::Listening);
    }

    #[test]
    fn core_transmit_channel_busy() {
        let _ = simplelog::SimpleLogger::init(log::LevelFilter::Debug, simplelog::Config::default());

        let packet = Packet::data(
            Address::Short(PanId(1), ShortAddress(2)), 
            Address::Short(PanId(1), ShortAddress(3)), 
            4, 
            &[0, 1, 2, 3, 4, 5]
        );

        let mut buff = vec![0u8; 1024];
        let _n = packet.encode(&mut buff, WriteFooter::No);

        let mut radio = MockRadio::new(&[]);
        let mut timer = MockTimer::new();
        let mut mac: Core<_, _, _, ()> = Core::new(radio.clone(), timer.clone(), vec![0u8; 128], AddressConfig::new(1, 2), CoreConfig::default());

        // Setup transmit_csma expectations
        radio.expect(&[
            // Check we're not currently busy
            Transaction::get_state(Ok(MockState::Idle)),
            // Enter receive mode for RSSI checking
            Transaction::start_receive(None),
            // Check noone else is (percievable) transmitting
            Transaction::poll_rssi(Ok(-20i16)),
        ]);

        timer.set_ms(1);

        // Try to start transmission
        assert_eq!(Ok(false), mac.transmit_csma(&packet));

        // Check expectations and state
        radio.done();
        assert_eq!(mac.state, CoreState::Listening);
    }
}
