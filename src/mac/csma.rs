
use core::fmt::Debug;

use log::{debug, trace, info, warn, error};

use radio::{Transmit, Receive, State, Busy, Rssi, ReceiveInfo};

use rand_core::RngCore;
use rand_facade::GlobalRng;

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
    Pending(u32, u32),
}

#[derive(Debug, PartialEq)]
pub struct CsmaConfig {
    /// Minimum number of backoffs prior to transmission
    pub min_backoff_count: u32,

    /// Maximum number of backoffs prior to transmission
    pub max_backof_count: u32,

    /// Backoff period in MS
    pub backoff_period_ms: u32,

    /// Maximum number of retries for un-acknow_msledged messages
    pub max_backoff_retries: u32,
}

impl Default for CsmaConfig {
    fn default() -> Self {
        Self {
            min_backoff_count: 1,
            max_backof_count: 5,
            backoff_period_ms: 10,
            max_backoff_retries: 2,
        }
    }
}

impl CsmaConfig {
    /// Generate a new random backoff time
    pub fn backoff_ms(&self) -> u32 {
        let mut backoff_slots = GlobalRng::get().next_u32() % self.max_backof_count;

        backoff_slots = backoff_slots.max(self.min_backoff_count);

        let backoff_time = backoff_slots * self.backoff_period_ms;

        backoff_time
    }
}

impl <R, I, E, T, B> Core<R, T, B, CsmaMode> 
where
    R: State<Error=E> + Busy<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E>,
    I: ReceiveInfo + Default,
    B: AsRef<[u8]> + AsMut<[u8]> + Debug,
    T: Timer,
{
    /// Create a new MAC using the provided radio
    pub fn new_csma(radio: R, timer: T, buffer: B, address: AddressConfig, csma_config: CsmaConfig, core_config: CoreConfig) -> Self {
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
            mode: CsmaMode {
                config: csma_config,
                state: CsmaState::Idle,
            },
        }
    }
}


impl <R, I, E, T, B> Mac for Core<R, T, B, CsmaMode> 
where
    R: State<Error=E> + Busy<Error=E> + Transmit<Error=E> + Receive<Info=I, Error=E> + Rssi<Error=E>,
    I: ReceiveInfo + Default + Debug,
    B: AsRef<[u8]> + AsMut<[u8]>,
    T: Timer,
{
    type Error = CoreError<E>;

    fn transmit(&mut self, packet: Packet) -> Result<(), Self::Error> {
        Core::set_transmit(self, packet)
    }

    fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        Core::get_received(self)
    }

    fn tick(&mut self) -> Result<Option<Packet>, Self::Error> {
        let now_ms = self.timer.ticks_ms();

        trace!("Tick at tick {} state: {:?}", now_ms, self.state);

        match self.state {
            CoreState::Idle => {
                debug!("Init, entering receive state");
                self.receive_start()?;
            },
            CoreState::Listening | CoreState::Receiving => {
                // Try to receive and unpack packet
                if let Some(packet) = self.try_receive()? {
                    self.handle_received(packet)?;
                    return Ok(None);
                }

                // If we're currently awaiting a TX, update CSMA state
                if let CsmaState::Pending(r, t) = self.mode.state {
                    let tx = self.tx_buffer.take().unwrap();

                    // If the backoff window is complete, transmit
                    if now_ms >= t {
                        debug!("Backoff expired at {} ms, starting tx", now_ms);

                        self.transmit_now(&tx)?;

                        self.tx_buffer = Some(tx);
                        return Ok(None)
                    }

                    // If the channel is busy, reset backoff window
                    if !self.channel_clear()? {
                        if r == 0 {
                            error!("Transmit timeout at {} ms, no remaining slots", now_ms);
                            self.mode.state = CsmaState::Idle;

                            return Err(CoreError::TransmitFailed(tx))
                        } else {
                            debug!("Backoff error at {} ms, retrying", now_ms);
                            let backoff_time = now_ms + self.mode.config.backoff_ms();
                            self.mode.state = CsmaState::Pending(r - 1, backoff_time);
                        }
                    }

                    // Otherwise, keep on
                    debug!("CSMA backoff ok at {} ms", now_ms);

                    self.tx_buffer = Some(tx);
                    return Ok(None)
                }

                // Arm TX
                if self.mode.state == CsmaState::Idle && self.tx_buffer.is_some() {
                    // Update retry count
                    self.retries = self.config.max_retries;

                    // Configure backoff
                    let backoff_rounds = self.mode.config.max_backoff_retries;
                    let backoff_time = now_ms + self.mode.config.backoff_ms();

                    self.mode.state = CsmaState::Pending(backoff_rounds, backoff_time);

                    debug!("Starting CSMA TX backoff at {} with expiry {}", now_ms, backoff_time);

                }
            },
            CoreState::Transmitting => {
                let done = self.transmit_done()?;
                if done {
                    // Reset CSMA state
                    self.mode.state = CsmaState::Idle;

                    let tx = self.tx_buffer.take().unwrap();

                    if !tx.header.ack_request {
                        debug!("Send complete");
                        return Ok(Some(tx));
                    } else {
                        debug!("Retrying TX");
                        self.tx_buffer = Some(tx);
                        self.retries -= 1;
                    }
                }
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
                if now_ms > (self.last_tick + self.config.ack_timeout_ms) {
                    let tx = self.tx_buffer.take().unwrap();

                    debug!("ACK timeout for packet {} at tick {}", tx.header.seq, now_ms);
                
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
    use std::vec;
    use ieee802154::mac::*;

    use radio::BasicInfo;
    use radio::mock::*;
    
    use crate::timer::mock::MockTimer;
    use super::*;


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
            Transaction::check_receive(true, Ok(false)),
            Transaction::get_state(Ok(MockState::Receive)),
            Transaction::poll_rssi(Ok(-90i16)),
            Transaction::check_receive(true, Ok(false)),
            Transaction::start_transmit((&buff[..n]).to_vec(), None),
        ]);

        // Queue packet for transmission
        timer.inc();
        mac.transmit(packet.clone()).unwrap();

        assert_eq!(mac.tx_buffer, Some(packet.clone()));

        info!("Starting TX");

        // Tick MAC to start RX
        timer.inc();
        mac.tick().unwrap();

        // Tick MAC to start TX backoff
        timer.inc();
        mac.tick().unwrap();

        assert_ne!(mac.mode.state, CsmaState::Idle);

        info!("CSMA started");

        // Override backoff time to next tick
        if let CsmaState::Pending(_r, t) = &mut mac.mode.state {
            *t = timer.val() + 2;
            info!("Overriding expiry to: {}", *t);
        }

        // Tick to poll rssi
        timer.inc();
        mac.tick().unwrap();

        // Tick to expire timer and start tx
        timer.inc();
        mac.tick().unwrap();


        // Check expectations and state
        assert_eq!(mac.state, CoreState::Transmitting);
        assert_eq!(mac.last_tick, timer.val());

        radio.done();


        // Setup transmit completion expectations

        radio.expect(&[
            Transaction::check_transmit(Ok(false)),
            Transaction::check_transmit(Ok(true)),
            Transaction::start_receive(None),
        ]);

        info!("Continuing TX");

        // Tick in transmitting state (still transmitting)
        timer.inc();
        mac.tick().unwrap();

        assert_eq!(mac.state, CoreState::Transmitting);
        assert_eq!(mac.last_tick, timer.val() - 1);

        info!("Completing TX");

        // Tick in transmitting state (transmission done)
        timer.inc();
        mac.tick().unwrap();

        // Check we're ready for an ACK
        assert_eq!(mac.state, CoreState::AwaitingAck);
        assert_eq!(mac.last_tick, timer.val());
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
