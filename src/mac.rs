

use core::{fmt::Debug, marker::PhantomData};


use log::{trace, debug, info, warn, error};

use ieee802154::mac::{Address, ExtendedAddress, FrameContent, PanId, WriteFooter};
use ieee802154::mac::beacon::{
    Beacon,
    BeaconOrder,
    PendingAddress,
    SuperframeOrder,
    SuperframeSpecification,
    GuaranteedTimeSlotInformation
};

use crate::{Radio, RawPacket, packet::Packet, error::CoreError, timer::Timer};

use crate::base::{Base, BaseState};

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub pan_coordinator: bool,
    pub pan_id: PanId,

    /// Base superframe duration in ms
    pub base_superframe_duration: u32,

    /// Mac beacon order, sets superframe length
    /// 
    /// beacon period = base_superframe_duration * 2^mac_beacon_order, 
    /// thus a value of 0 sets the superframe length to base_superframe_duration
    /// Valid values are 0 < v < 15, a value of 15 disables sending beacon frames
    pub mac_beacon_order: BeaconOrder,

    /// Mac superframe order (ie. how much of that superframe is active)
    ///
    /// SD = base_superframe_duration * 2^mac_superframe_order,
    /// thus for a mac_beacon_order of 1, a mac_superframe_order of 0 would
    /// be of 2*base_superframe_duration length with a base_superframe_duration active period.
    /// Valid values are 0 < v < 15, a value of 15 disables the whole superframe
    pub mac_superframe_order: SuperframeOrder,

    /// Base slot duration in ms
    pub base_slot_duration: u32,

    /// Maximum number of retries
    pub max_retries: u8,
    /// Minimum backoff exponent
    pub min_be: u8,
    /// Maximum backoff exponent
    pub max_be: u8,


    /// Deadline for MAC operations (maximum allowed schedule slip)
    pub mac_deadline: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pan_coordinator: false,
            pan_id: PanId(0x0100),

            base_superframe_duration: 1000,
            base_slot_duration: 100,

            mac_beacon_order: BeaconOrder::BeaconOrder(1),
            mac_superframe_order: SuperframeOrder::SuperframeOrder(0),
            mac_deadline: 2,

            max_retries: 5,
            min_be: 1,
            max_be: 5,
        }
    }
}

impl Config {
    pub fn superframe_duration(&self) -> u32 {
        match self.mac_beacon_order {
            BeaconOrder::BeaconOrder(o) => {
                (self.base_superframe_duration * 2_u32.pow(o as u32)) as u32
            },
            _ => 0,
        }
    }

    pub fn superframe_spec(&self) -> SuperframeSpecification {
        SuperframeSpecification {
            beacon_order: self.mac_beacon_order,
            superframe_order: self.mac_superframe_order,
            pan_coordinator: self.pan_coordinator,
            // TODO: these values are placeholders and need to be correctly set
            battery_life_extension: false,
            association_permit: true,
            final_cap_slot: 0,
        }
    }

    pub fn calculate_sfn(&self, now: u64, offset: u64) -> u64 {
        (now + offset) / self.superframe_duration() as u64
    }

    pub fn calculate_asn(&self, now: u64, offset: u64) -> u64 {
        (now + offset) / self.base_slot_duration as u64
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MacState {
    Idle,
    Sleep,
    Beacon,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncState {
    Unsynced,
    Synced(Address),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssocState {
    Pending,
    Associated(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mac<R, S, I, E, T> {
    pub address: ExtendedAddress,
    config: Config,
    base: Base<R, S, I, E>,
    timer: T,

    seq: u8,
    sync_offset: u64,
    last_asn: u64,
    next_beacon: u64,

    sync_state: SyncState,
}

impl <R, S, I, E, T> Mac<R, S, I, E, T> 
where
    R: Radio<S, I, E>,
    S: radio::RadioState,
    I: radio::ReceiveInfo + Default + Debug,
    E: Debug,
    T: Timer,
{
    pub fn new(address: ExtendedAddress, config: Config, radio: R, timer: T) -> Result<Self, CoreError<E>> {
        let mut s = Self {
            address,
            config,

            base: Base::new(radio)?,
            timer,
            
            seq: 0,
            sync_offset: 0,
            last_asn: 0,
            next_beacon: 0,
            sync_state: SyncState::Unsynced,
        };

        let now = s.timer.ticks_ms();
        s.sync_offset = now;

        debug!("Setup MAC at {} ms", now);

        if s.config.pan_coordinator && s.config.mac_beacon_order != BeaconOrder::OnDemand {
            s.next_beacon = now + s.config.superframe_duration() as u64;
            debug!("Setup next beacon for {} ms", s.next_beacon);
        }

        debug!("Set radio to receive mode");
        s.base.receive(now)?;

        Ok(s)
    }

    pub fn tick(&mut self) -> Result<(), CoreError<E>> {

        let now_ms = self.timer.ticks_ms();
        
        let last_sync_state = self.sync_state.clone();

        let sfn = self.config.calculate_sfn(now_ms, self.sync_offset);
        let asn = self.config.calculate_asn(now_ms, self.sync_offset);

        trace!("Tick at {} ms with ASN: {} (SFN: {})", now_ms, asn, sfn);

        // Update base radio interface
        if let Some(rx) = self.base.tick(now_ms)? {
            // Handle received packets
            self.handle_received(now_ms, rx)?;
        }

        // Receive and broadcast beacon at configured intervals
        
        // TODO: add time compensation to prepare radio prior to timeslot
        // (wake early and busy-wait for appropriate TX, start RX ahead of time)
        
        // TODO: add shift for non-pan-coordinator beaconing

        // TODO: could this be refactored to be -asn- specific rather than time,
        // and would that be better?

        if self.next_beacon != 0 && self.next_beacon <= now_ms {
            // Check for beacon schedule misses
            if (self.next_beacon + self.config.mac_deadline as u64) < now_ms {
                warn!("MAC deadline exceeded");
            }

            // PAN coordinator broadcasts beacons
            // TODO: as do other coordinators in their respective slots? need to tx and rx for these
            // TODO: is there still a randomness to this to avoid collisions if neighbors > slotframe count?
            if self.config.pan_coordinator {
                debug!("Broadcasting beacon in ASN: {} at {}", asn, now_ms);

                // TODO: beacon type varies with TSCH/non-tsch?
                let beacon = Beacon {
                    superframe_spec: self.config.superframe_spec(),
                    // TODO: replace placeholders with actual configuration
                    guaranteed_time_slot_info: GuaranteedTimeSlotInformation::new(),
                    pending_address: PendingAddress::new(),
                };

                let addr = Address::Extended(self.config.pan_id, self.address);
                let packet = Packet::beacon(addr, self.seq, beacon);
                self.seq += 1;

                let mut buff = [0u8; 256];
                let n = packet.encode(&mut buff, WriteFooter::No);

                self.base.transmit(now_ms, &buff[..n])?;

                // Re-arm beacon for next slot
                self.next_beacon += self.config.superframe_duration() as u64;

                debug!("Armed next beacon TX for {} ms", self.next_beacon);

            } else {
                debug!("Arming next beacon RX for ASN: {} at {} ms", asn, now_ms);

                if self.base.state() != BaseState::Listening {
                    self.base.receive(now_ms)?;
                }

                // TODO: re-arm beacon or keep listening depending on join state?
                // This has to happen _after_ rx I guess
                // so we need a timeout on operations? or maybe on slots?
            }
        }


        // TODO: Handle state changes
        if self.sync_state != last_sync_state {
            
        }

        Ok(())
    }

    fn handle_received(&mut self, now: u64, rx: RawPacket) -> Result<(), CoreError<E>> {

        // Decode packet
        let p = match Packet::decode(rx.data(), false) {
            Ok(p) => p,
            Err(e) => {
                error!("Error decoding received packet: {:?}", e);
                return Err(CoreError::DecodeError(e))
            }
        };

        trace!("Received packet: {:?}", p);

        // TODO: filter by pan ID depending on filters / network state

        // Handle received packets
        match p.content {
            FrameContent::Beacon(b) => {
                debug!("Received beacon from {:?} at {} ms", p.header.source, now);

                // If we're the pan coordinator we're not going to _sync_ on this
                // (but it might be useful to look at for drift?)
                if self.config.pan_coordinator {

                // If we're unsynced parse this and decide whether to adopt as the
                // authorative time source
                } else if self.sync_state == SyncState::Unsynced {

                    debug!("Adopting sync parent {:?}", p.header.source);

                    // TODO: apply received configuration

                    // Set sync state and compute next beacon time
                    // TODO: apply shift to compensate for time to tx/rx beacon
                    self.sync_state = SyncState::Synced(p.header.source);
                    // TODO: in TSCH impls sync offset set based on ASN
                    self.sync_offset = now;
                    self.next_beacon = now + self.config.superframe_duration() as u64;

                // If we're synced use this to evaluate drift and correct _if_ it's from
                //our parent
                } else if let SyncState::Synced(parent) = self.sync_state {
                    if p.header.source != parent {
                        debug!("Disgarding sync from non-parent: {:?}", p.header.source);

                    } else {
                        // Compute offset from expected time
                        // This is improved by TSCH EBs / ASNs huh?
                        // TODO: what happens if we're > one slot out of sync
                        let offset = now as i64 - self.next_beacon as i64;

                        debug!("Received new beacon at {} ms (expected at {} ms, offset: {} ms)",
                            now, self.next_beacon, offset);
                        
                        // Update stack synchronization offset
                        // TODO: improve this to a piecewise / averaging offset correction
                        self.sync_offset = offset as u64 % self.config.superframe_duration() as u64;

                        // Set new beacon time
                        // TODO: really this should happen in tick rather than here?
                        self.next_beacon = now + self.config.superframe_duration() as u64;
                        debug!("Arm next beacon at {} ms", self.next_beacon);
                    }
                }

                // TODO: apply beacon info
            },
            FrameContent::Command(c) => {

            },
            FrameContent::Acknowledgement => {

            },
            FrameContent::Data => {

            },
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::vec;

    use ieee802154::mac::*;
    use radio::{BasicInfo, mock::*};
    
    use crate::timer::mock::MockTimer;
    use super::*;

    #[test]
    fn beacon_tx() {
        let _ = simplelog::SimpleLogger::init(log::LevelFilter::Info, simplelog::Config::default());
        
        let mut radio = MockRadio::new(&[]);
        let mut timer = MockTimer::new();
        let mut seq = 0;

        let mac_addr = ExtendedAddress(0xabcd);
        let mac_cfg = Config{
            pan_coordinator: true,
            ..Default::default()
        };

        // Initialise MAC
        radio.expect(&[
            Transaction::start_receive(None),
        ]);
        let mut mac = Mac::new(mac_addr.clone(), mac_cfg.clone(), radio.clone(), timer.clone()).unwrap();

        // Chilling in rx mode
        radio.expect(&[
            Transaction::check_receive(true, Ok(false)),
        ]);
        mac.tick().unwrap();

        for n in 0..2 {

            // Advance in time
            for i in 0..mac_cfg.superframe_duration() / 100 - 1 {
                timer.set_ms(n * mac_cfg.superframe_duration() + i * 100);

                radio.expect(&[
                    Transaction::check_receive(true, Ok(false)),
                ]);

                mac.tick().unwrap();
            }

            // Beacon at beacon interval
            timer.set_ms((n + 1) * mac_cfg.superframe_duration());

            let beacon_info = Beacon {
                superframe_spec: mac_cfg.superframe_spec(),
                // TODO: replace placeholders with actual configuration
                guaranteed_time_slot_info: GuaranteedTimeSlotInformation::new(),
                pending_address: PendingAddress::new(),
            };
            let beacon = Packet::beacon(Address::Extended(mac_cfg.pan_id, mac_addr), seq, beacon_info);
            seq += 1;

            // Start beacon TX
            radio.expect(&[
                Transaction::check_receive(true, Ok(false)),
                Transaction::start_transmit(beacon.into(), None),
            ]);
            mac.tick().unwrap();

            // Complete beacon TX
            radio.expect(&[
                Transaction::check_transmit(Ok(true)),
                Transaction::start_receive(None),
            ]);
            mac.tick().unwrap();

        }
    }

    #[test]
    fn beacon_rx_sync() {
        let _ = simplelog::SimpleLogger::init(log::LevelFilter::Debug, simplelog::Config::default());
        
        let mut radio = MockRadio::new(&[]);
        let mut timer = MockTimer::new();
        let mut seq = 0;

        let mac_addr = ExtendedAddress(0xabcd);
        let mac_cfg = Config{
            pan_coordinator: false,
            ..Default::default()
        };
        let coord_addr = Address::Extended(mac_cfg.pan_id, ExtendedAddress(0x1122));


        // Initialise MAC
        radio.expect(&[
            Transaction::start_receive(None),
        ]);
        let mut mac = Mac::new(mac_addr.clone(), mac_cfg.clone(), radio.clone(), timer.clone()).unwrap();

        // Chilling in rx mode
        radio.expect(&[
            Transaction::check_receive(true, Ok(false)),
        ]);
        mac.tick().unwrap();

        
        timer.set_ms(100);

        debug!("RX beacon");

        // Receive beacon
        let beacon_info = Beacon {
            superframe_spec: mac_cfg.superframe_spec(),
            // TODO: replace placeholders with actual configuration
            guaranteed_time_slot_info: GuaranteedTimeSlotInformation::new(),
            pending_address: PendingAddress::new(),
        };
        let beacon = Packet::beacon(coord_addr, seq, beacon_info);
        seq += 1;

        radio.expect(&[
            Transaction::check_receive(true, Ok(true)),
            Transaction::get_received(Ok((beacon.into(), BasicInfo::default()))),
            Transaction::start_receive(None),
        ]);
        mac.tick().unwrap();


        // Check sync offset / next beacon time applied
        assert_eq!(mac.sync_state, SyncState::Synced(coord_addr));
        assert_eq!(mac.next_beacon, mac_cfg.superframe_duration() as u64 + timer.ticks_ms());

    }

    #[test]
    fn beacon_rx_next() {
        let _ = simplelog::SimpleLogger::init(log::LevelFilter::Debug, simplelog::Config::default());
        
        let mut radio = MockRadio::new(&[]);
        let mut timer = MockTimer::new();
        let mut seq = 0;

        let mac_addr = ExtendedAddress(0xabcd);
        let mac_cfg = Config{
            pan_coordinator: false,
            ..Default::default()
        };
        let coord_addr = Address::Extended(mac_cfg.pan_id, ExtendedAddress(0x1122));


        // Initialise MAC
        radio.expect(&[
            Transaction::start_receive(None),
        ]);
        let mut mac = Mac::new(mac_addr.clone(), mac_cfg.clone(), radio.clone(), timer.clone()).unwrap();

        // Chilling in rx mode, force to sleep so we see wake -> RX transition
        radio.expect(&[
            Transaction::check_receive(true, Ok(false)),
            Transaction::set_state(MockState::Sleep, None),
        ]);
        mac.tick().unwrap();
        mac.base.sleep().unwrap();


        // Set sync'd state so we're expecting a beacon
        mac.sync_state = SyncState::Synced(coord_addr.clone());
        mac.next_beacon = mac_cfg.superframe_duration() as u64;

        // Arm RX for next expected beacon
        timer.set_ms(mac.next_beacon as u32 + 3);
        radio.expect(&[
            Transaction::start_receive(None),
        ]);
        mac.tick().unwrap();

        debug!("RX beacon");

        // Receive beacon
        let beacon_info = Beacon {
            superframe_spec: mac_cfg.superframe_spec(),
            // TODO: replace placeholders with actual configuration
            guaranteed_time_slot_info: GuaranteedTimeSlotInformation::new(),
            pending_address: PendingAddress::new(),
        };
        let beacon = Packet::beacon(coord_addr, seq, beacon_info);
        seq += 1;

        radio.expect(&[
            Transaction::check_receive(true, Ok(true)),
            Transaction::get_received(Ok((beacon.into(), BasicInfo::default()))),
            Transaction::start_receive(None),
        ]);
        mac.tick().unwrap();


        // Check sync offset / next beacon time applied
        assert_eq!(mac.sync_state, SyncState::Synced(coord_addr));
        assert_eq!(mac.next_beacon, mac_cfg.superframe_duration() as u64 + timer.ticks_ms());

    }
}
