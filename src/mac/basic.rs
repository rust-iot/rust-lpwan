
use core::fmt::Debug;

use log::{debug, info, warn, error};

use ieee802154::mac::*;
use radio::{Transmit, Receive, State, Rssi, ReceiveInfo, IsBusy};

use crate::{timer::Timer, mac::Mac, packet::Packet};

use super::config::*;
use super::error::*;
use super::core::*;


#[derive(Debug, PartialEq)]
pub struct CsmaMode {
    config: CsmaConfig,
    state: CsmaState,
}

#[derive(Debug, PartialEq)]
pub enum CsmaState {
    Idle,
}

#[derive(Debug, PartialEq)]
pub struct CsmaConfig {

}

impl Default for CsmaConfig {
    fn default() -> Self {
        Self{

        }
    }
}

impl <R, I, E, T, B> Core<R, T, B, CsmaMode> 
where
    R: State<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E> + Debug,
    I: ReceiveInfo + Default + Debug,
    B: AsRef<[u8]> + AsMut<[u8]> + Debug,
    T: Timer + Debug,
{
    /// Create a new MAC using the provided radio
    pub fn new_csma(radio: R, timer: T, buffer: B, address: AddressConfig, csma_config: CsmaConfig, core_config: CoreConfig) -> Self {
        Self{
            address,
            config: core_config,

            state: CoreState::Idle,
            
            ack_required: false,
            retries: 0,

            last_tick: timer.ticks_ms(),

            buffer,

            rx_buffer: None,
            tx_buffer: None,

            timer,
            radio,
            mode: CsmaMode {
                config: csma_config,
                state: CsmaState::Idle,
            },
        }
    }
}


impl <R, I, E, T, B> Mac for Core<R, T, B, CsmaMode> 
where
    R: State<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E> + Debug,
    I: ReceiveInfo + Default + Debug,
    B: AsRef<[u8]> + AsMut<[u8]> + Debug,
    T: Timer + Debug,
{
    type Error = CoreError<E>;

    fn transmit(&mut self, packet: Packet) -> Result<(), Self::Error> {
        Core::set_transmit(self, packet)
    }

    fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        Core::get_received(self)
    }

    fn tick(&mut self) -> Result<Option<Packet>, Self::Error> {
        let now = self.timer.ticks_ms();

        debug!("Tick at tick {} state: {:?}", now, self.state);

        match self.state {
            CoreState::Idle => {
                debug!("Init, entering receive state");
                self.receive_start()?;
            },
            CoreState::Listening | CoreState::Receiving => {
                // Try to receive and unpack packet
                if let Some(packet) = self.try_receive()? {
                    self.handle_received(packet)?;
                }

                // If we're still in receive mode (not sending an ACK)
                // and we have a packet ready to send, start the backoff
                // TODO: make this random
                

                // Send packet if pending
                if let Some(tx) = self.tx_buffer.take() {
                    debug!("Attempting transmission");

                    // TODO: setup ack /retry info

                    let res = self.transmit_csma(&tx);
                    
                    self.tx_buffer = Some(tx);
                    let _ = res?;
                }
            },
            CoreState::Transmitting => {
                self.transmit_done()?;
            },
            CoreState::AwaitingAck => {
                 // Receive packets if available
                let incoming = match self.try_receive()? {
                    Some(v) => v,
                    None => return Ok(None),
                };

                // Handle incoming ACK packets
                let acked = self.handle_ack(incoming)?;
                if acked {
                    return Ok(None);
                }

                // Timeout ACKs
                if now > (self.last_tick + self.config.ack_timeout_ms) {
                    let tx = self.tx_buffer.take().unwrap();

                    debug!("ACK timeout for packet {} at tick {}", tx.header.seq, now);
                
                    if self.retries > 0 {
                        // Update retries and re-attempt transmission
                        
                        self.transmit_csma(&tx)?;

                        self.retries -= 1;
                        self.tx_buffer = Some(tx);

                    } else {
                        // Restart receive mode
                        self.receive_start()?;

                        // Return transmit error
                        return Err(CoreError::TransmitFailed(tx))
                    }
                
                }
            },
            CoreState::Sleeping => {

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
    fn transmit_with_ack() {
        let _ = simplelog::SimpleLogger::init(log::LevelFilter::Debug, simplelog::Config::default());

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
        let mut mac: Core<_, _, _, CsmaMode> = Core::new_csma(radio.clone(), timer.clone(), vec![0u8; 128], AddressConfig::new(1, 2), CsmaConfig::default(), CoreConfig::default());

        // Setup transmit_csma expectations
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
        assert_eq!(mac.state, CoreState::Transmitting);
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

        assert_eq!(mac.state, CoreState::Transmitting);
        assert_eq!(mac.last_tick, 3);

        info!("Completing TX");

        // Tick in transmitting state (transmission done)
        timer.set_ms(5);
        mac.tick().unwrap();

        // Check we're ready for an ACK
        assert_eq!(mac.state, CoreState::AwaitingAck);
        assert_eq!(mac.last_tick, 5);
        assert_eq!(mac.ack_required, true);
        assert_eq!(mac.retries, mac.config.max_retries);
        assert_eq!(mac.tx_buffer, Some(packet.clone()));

        radio.done();
    }

    #[test]
    fn ack_receive() {
        let _ = simplelog::SimpleLogger::init(log::LevelFilter::Debug, simplelog::Config::default());


        let mut packet = Packet::data(
            Address::Short(PanId(1), ShortAddress(2)), 
            Address::Short(PanId(1), ShortAddress(3)), 
            4, 
            &[0, 1, 2, 3, 4, 5]
        );
        packet.header.ack_request = true;

        let mut radio = MockRadio::new(&[]);
        let mut timer = MockTimer::new();
        let mut mac: Core<_, _, _, CsmaMode> = Core::new_csma(radio.clone(), timer.clone(), vec![0u8; 128], AddressConfig::new(1, 2), CsmaConfig::default(), CoreConfig::default());

        // Configure MAC into AwaitingAck state
        mac.state = CoreState::AwaitingAck;
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

        assert_eq!(mac.state, CoreState::AwaitingAck);
        assert_eq!(mac.last_tick, 0);

        // Second tick, Receive ack
        timer.set_ms(2);
        mac.tick().unwrap();

        // Return to receive mode
        assert_eq!(mac.state, CoreState::Listening);
        assert_eq!(mac.last_tick, 2);

        radio.done();
    }
}
