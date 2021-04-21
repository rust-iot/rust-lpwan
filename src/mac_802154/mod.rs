//! 802.15.4 MAC Implementation
//
// https://github.com/rust-iot/rust-lpwan
// Copyright 2021 Ryan Kurte

use core::{fmt::Debug};

use ieee802154::mac::{Address, ExtendedAddress, FrameContent, PanId, ShortAddress, WriteFooter};
use ieee802154::mac::beacon::{
    Beacon,
    BeaconOrder,
    PendingAddress,
    GuaranteedTimeSlotInformation
};
use ieee802154::mac::command::{
    Command,
    CapabilityInformation,
    AssociationStatus,
};


use crate::log::{trace, debug, info, warn, error};
use heapless::{spsc::Queue, consts::U16};

use rand_core::RngCore;
use rand_facade::{GlobalRng};

use crate::{Mac as MacIf, Radio, RawPacket, RxInfo, error::CoreError, timer::Timer};
use crate::base::{Base, BaseState};

pub mod config;
pub use config::Config;

pub mod packet;
pub use packet::Packet;

pub mod channels;


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

impl SyncState {
    pub fn is_synced(&self) -> bool {
        match self {
            SyncState::Synced(_) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssocState {
    Unassociated,
    Pending(Address, u64),
    Associated(PanId),
}

impl AssocState {
    pub fn is_associated(&self) -> bool {
        match self {
            AssocState::Associated(_) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TxState {
    pub pending: bool,
    pub retries: u8,
}

impl Default for TxState {
    fn default() -> Self {
        Self {
            pending: true,
            retries: 0,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum CsmaState {
    None,
    Pending {
        packet: Packet,
        tx_slot: u64,
        retries: u64,
    },
}


#[derive(Debug, Clone, PartialEq)]
pub enum AckState {
    None,
    Pending {
        packet: Packet,
        tx_time: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum MacEvent {

}


#[derive(Debug, Clone, PartialEq)]
pub struct MacStats {
    pub deadline_miss_tx: u32,
    pub deadline_miss_ack: u32,
    pub csma_cca_fail: u32,
    pub tx_fail: u32,
    pub sync_fail: u32,
}

impl MacStats  {
    pub fn new() -> Self {
        Self {
            deadline_miss_tx: 0,
            deadline_miss_ack: 0,
            csma_cca_fail: 0,
            tx_fail: 0,
            sync_fail: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mac<R, S, I, E, T> {
    pub address: ExtendedAddress,
    pub short_addr: Option<ShortAddress>,

    config: Config,
    base: Base<R, S, I, E>,
    timer: T,

    seq: u8,
    sync_offset: u64,
    last_asn: u64,

    next_beacon: u64,
    beacon_miss_count: u32,

    sync_state: SyncState,
    assoc_state: AssocState,
    csma_state: CsmaState,
    ack_state: AckState,

    stats: MacStats,

    rx_buff: Queue<(RxInfo, Packet), U16>,
    tx_buff: Queue<(TxState, Packet), U16>,
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
            short_addr: None,
            config,

            base: Base::new(radio)?,
            timer,
            
            seq: 0,
            sync_offset: 0,
            last_asn: 0,
            next_beacon: 0,
            beacon_miss_count: 0,

            sync_state: SyncState::Unsynced,
            assoc_state: AssocState::Unassociated,
            csma_state: CsmaState::None,
            ack_state: AckState::None,

            stats: MacStats::new(),

            rx_buff: Queue::new(),
            tx_buff: Queue::new(),
        };

        let now = s.timer.ticks_ms();
        s.sync_offset = now;

        debug!("Setup MAC with address {:?} at {} ms", s.address, now);

        if s.config.pan_coordinator && s.config.mac_beacon_order != BeaconOrder::OnDemand {
            s.next_beacon = now + s.config.superframe_duration() as u64;
            debug!("Setup next beacon for {} ms", s.next_beacon);
        }

        if s.config.pan_coordinator {
            s.assoc_state = AssocState::Associated(s.config.pan_id);
        }

        debug!("Set radio to receive mode");
        s.base.receive(now)?;

        Ok(s)
    }
}

impl <R, S, I, E, T> MacIf<Address> for Mac<R, S, I, E, T> 
where
    R: Radio<S, I, E>,
    S: radio::RadioState,
    I: radio::ReceiveInfo + Default + Debug,
    E: Debug,
    T: Timer,
{
    type Error = CoreError<E>;

    /// Enqueue a packet for TX
    fn transmit(&mut self, dest: Address, data: &[u8], ack: bool) -> Result<(), Self::Error> {
        // Setup packet for sending
        let packet = Packet::data(dest, self.addr(), self.seq(), data, ack);

        // Enqueue in TX buffer
        if let Err(_e) = self.tx_buff.enqueue((TxState::default(), packet)) {
            error!("Error enqueuing packet to send");
        }

        Ok(())
    }

    /// Check for received packets
    fn receive(&mut self, data: &mut[u8]) -> Result<Option<(usize, RxInfo)>, Self::Error> {
        // Fetch from RX buffer
        let rx = match self.rx_buff.dequeue() {
            Some(rx) => rx,
            None => return Ok(None)
        };

        // Decode data
        let payload = rx.1.payload();
        &data[..payload.len()].copy_from_slice(&payload);

        // Return payload length
        Ok(Some((payload.len(), rx.0)))
    }

    /// Check whether the MAC is busy
    fn busy(&mut self) -> Result<bool, Self::Error> {
        let b =self.csma_state != CsmaState::None
            || self.ack_state != AckState::None
            || !self.assoc_state.is_associated()
            || self.tx_buff.capacity() == 0;

        Ok(b)
    }

    fn tick(&mut self) -> Result<(), Self::Error> {
        let now_ms = self.timer.ticks_ms();
        
        let last_sync_state = self.sync_state.clone();

        let sfn = self.config.calculate_sfn(now_ms, self.sync_offset);
        let asn = self.config.calculate_asn(now_ms, self.sync_offset);
        let rsn = self.config.calculate_rsn(now_ms, self.sync_offset);

        trace!("Tick at {} ms with ASN: {} (SFN: {} RSN: {})", now_ms, asn, sfn, rsn);

        // Update base radio interface
        // TODO: come up with a mechanism for propagating radio state changes 
        // so we don't have to always poll on the radio?
        if let Some(rx) = self.base.tick(now_ms)? {
            // Handle received packets
            self.handle_received(now_ms, rx)?;
        }

        // Compute state based on slot
        // TODO: refactor this out so that the slot selector can be unit tested
        
        // TODO: add time compensation to prepare radio prior to timeslot
        // (wake early and busy-wait for appropriate TX, start RX ahead of time)
        
        // TODO: add shift for non-pan-coordinator beaconing

        // TODO: could this be refactored to be -asn- specific rather than time,
        // and would that be better?


        // Standard beacon takes place in the first slot
        if rsn == 0 {    
            self.tick_beacon(now_ms, asn)?;
        }

        // Transmit ACKs if scheduled
        match self.ack_state.clone() {
            AckState::Pending{packet, tx_time} if tx_time < now_ms => {
                if now_ms > (tx_time + self.config.mac_deadline as u64) {
                    warn!("ACK deadline exceeded (expected: {} actual: {})", tx_time, now_ms);
                    self.stats.deadline_miss_ack = self.stats.deadline_miss_ack.saturating_add(1);
                }

                debug!("Sending ACK for packet {} from {:?} at {} ms", packet.header.seq, packet.header.destination, now_ms);

                let mut buff = [0u8; 256];
                let n = packet.encode(&mut buff, WriteFooter::No);

                self.base.transmit(now_ms, &buff[..n])?;

                self.ack_state = AckState::None;
            },
            _ => (),
        }

        // TODO: CSMA operations take place during Contention Access Period (CAP), starting from the beacon frame
        self.tick_cap(now_ms, asn)?;

        // TODO: Collision free operations occupy the rest of the slot

    

        // TODO: Handle state changes
        match (self.sync_state.clone(), self.assoc_state.clone()) {
            // On sync, attempt association
            (SyncState::Synced(parent), AssocState::Unassociated) => {

                let assoc_cmd = Command::AssociationRequest(CapabilityInformation{
                    // TODO: entirely placeholders, update from config
                    allocate_address: true,
                    frame_protection: false,
                    full_function_device: true,
                    mains_power: false,
                    idle_receive: false,
                });

                let assoc = Packet::command(parent, self.addr(), self.seq(), assoc_cmd);

                // TODO: handle error
                if let Err(_) = self.tx_buff.enqueue((TxState::default(), assoc)) {
                    error!("Error adding associate request to tx buffer");
                }

                info!("Received network sync, issuing association request");

                self.assoc_state = AssocState::Pending(parent.clone(), now_ms + self.config.assoc_timeout);
            },
            // Timeout pending associations
            (SyncState::Synced(_parent), AssocState::Pending(_assoc_parent, expiry)) => {
                if now_ms > expiry {
                    warn!("Association request expired at {} ms", now_ms);
                    // TODO: association backoff? forced de-sync to retry?
                    self.assoc_state = AssocState::Unassociated;
                }
            },
            // IDK
            (SyncState::Synced(_parent), AssocState::Associated(_pan_id)) => {

            },
            // Drop association on de-sync?
            // TODO: do we want to do this or, attempt to re-sync first?
            (SyncState::Unsynced, AssocState::Associated(_pan_id)) if last_sync_state != SyncState::Unsynced => {
                self.stats.sync_fail = self.stats.sync_fail.saturating_add(1);
                self.assoc_state = AssocState::Unassociated;
            }
            _ => (),
        }

        Ok(())
    }
}

impl <R, S, I, E, T> Mac<R, S, I, E, T> 
where
    R: Radio<S, I, E>,
    S: radio::RadioState,
    I: radio::ReceiveInfo + Default + Debug,
    E: Debug,
    T: Timer,
{
    /// Fetch configured MAC address
    pub fn addr(&self) -> Address {
        // TODO: use broadcast(?) pan_id if unjoined
        match self.short_addr {
            Some(s) => Address::Short(self.config.pan_id, s),
            None => Address::Extended(self.config.pan_id, self.address),
        }
    }

    /// Fetch MAC state
    pub fn state(&self) -> (SyncState, AssocState) {
        (self.sync_state.clone(), self.assoc_state.clone())
    }

    /// Fetch and increment TX sequence number
    fn seq(&mut self) -> u8 {
        let s = self.seq;
        self.seq = self.seq.wrapping_add(1);
        s
    }

    /// Fetch MAC layer statistics
    pub fn stats(&self) -> MacStats {
        self.stats.clone()
    }

    fn tick_beacon(&mut self, now_ms: u64, asn: u64) -> Result<(), CoreError<E>> {

        // No ASN change / nothing we need to do for beaconing
        if self.last_asn == asn {
            return Ok(())
        }

        // No pending beacon or not yet expected beacon time
        if self.next_beacon == 0 || self.next_beacon > now_ms {
            return Ok(())
        }

        // Check for schedule misses
        // (self.next_beacon updated on receipt of viable beacon)
        if (self.next_beacon + self.config.mac_deadline as u64) < now_ms {

            // Desync after configured number of beacon misses
            if let SyncState::Synced(_) = self.sync_state {
                self.beacon_miss_count += 1;

                if self.beacon_miss_count > self.config.max_beacon_misses {
                    warn!("Exceeded maximum beacon misses, synchronization lost");
                    self.sync_state = SyncState::Unsynced;
                    self.next_beacon = 0;
    
                    return Ok(());
                }
            } else {
                // TODO: Count coordinator schedule misses here
            }
        }

        // PAN coordinator broadcasts beacons
        // TODO: as do other coordinators in their respective slots? need to tx and rx for these
        // TODO: is there still a randomness to this to avoid collisions if neighbors > slotframe count?
        if self.config.pan_coordinator {
            debug!("Broadcasting beacon in ASN: {} at {} ms", asn, now_ms);

            // TODO: beacon type varies with TSCH/non-tsch?
            let beacon = Beacon {
                superframe_spec: self.config.superframe_spec(),
                // TODO: replace placeholders with actual configuration
                guaranteed_time_slot_info: GuaranteedTimeSlotInformation::new(),
                pending_address: PendingAddress::new(),
            };

            let packet = Packet::beacon(self.addr(), self.seq(), beacon);

            let mut buff = [0u8; 256];
            let n = packet.encode(&mut buff, WriteFooter::No);

            self.base.transmit(now_ms, &buff[..n])?;

            // Re-arm beacon for next slot
            self.next_beacon += self.config.superframe_duration() as u64;

            debug!("Armed next beacon TX for {} ms", self.next_beacon);

        } else {

            debug!("Set beacon RX for ASN: {} at {} ms", asn, now_ms);

            if self.base.state() != BaseState::Listening {
                self.base.receive(now_ms)?;
            }

            // TODO: re-arm beacon or keep listening depending on join state?
            // This has to happen _after_ rx I guess
            // so we need a timeout on operations? or maybe on slots?

            self.next_beacon += self.config.superframe_duration() as u64;
            debug!("Arm next beacon RX for {} ms", self.next_beacon);
        }

        Ok(())
    }

    fn tick_cap(&mut self, now_ms: u64, asn: u64) -> Result<(), CoreError<E>> {
        let rsn = self.config.calculate_rsn(now_ms, self.sync_offset);

        if asn != self.last_asn && rsn == 0 {
            // If we're already attempting CSMA, restart if possible
            if let CsmaState::Pending{packet, tx_slot, retries} = &self.csma_state {
                // Limit CSMA backoff retries
                if *retries >= self.config.csma_max_backoffs as u64 {
                    warn!("CSMA TX failed for packet {}", packet.header.seq);
                    self.stats.csma_cca_fail = self.stats.csma_cca_fail.saturating_add(1);

                    // TODO: should _mac_ ACK/Retry cause CSMA re-attempts?

                    // TODO: notify higher level of failure?
                    self.csma_state = CsmaState::None;
                    let _ = self.tx_buff.dequeue();

                } else if *tx_slot == 0 {
                    // Re-schedule CSMA attempt
                    let be = (self.config.min_be as u32 + *retries as u32).min(self.config.max_be as u32);

                    let backoff = (GlobalRng::get().next_u32() % (2u32.pow(be as u32) - 1)) as u64 + 1;

                    debug!("Scheduling CSMA TX retry for ASN {} ({} slots)", asn + backoff, backoff);

                    self.csma_state = CsmaState::Pending{
                        packet: packet.clone(),
                        tx_slot: asn + backoff,
                        retries: *retries + 1,
                    };
                }

            // Otherwise if we have something to TX, get started
            } else if let Some(tx) = self.tx_buff.peek().map(|v| v.clone() ) {
                debug!("Found pending packet {} to: {:?}", tx.1.header.seq, tx.1.header.destination);

                // Check TX retries and increase counter
                if tx.0.retries > self.config.max_retries {
                    debug!("Packet {} TX failed exceeded max retries", tx.1.header.seq);
                    self.stats.tx_fail = self.stats.tx_fail.saturating_add(1);

                    let _ = self.tx_buff.dequeue();
                    return Ok(())
                }
                self.tx_buff.iter_mut()
                    .find(|(_, p)| p.header.seq == tx.1.header.seq )
                    .map(|(i, _)| i.retries += 1 ); 

                // Calcuate backoff periods for TX
                let be = match self.config.battery_life_extension {
                    true => 2.min(self.config.min_be),
                    false => self.config.min_be,
                };

                let backoff = (GlobalRng::get().next_u32() % (2u32.pow(be as u32) - 1)) as u64 + 1;

                debug!("Scheduling CSMA TX for ASN {} ({} slots)", asn + backoff, backoff);

                self.csma_state = CsmaState::Pending{
                    packet: tx.1.clone(),
                    tx_slot: asn + backoff,
                    retries: 0,
                };
            }
        
        // In other slots _if_ we have a pending TX, run CSMA
        } else if let CsmaState::Pending{packet, tx_slot, retries} = self.csma_state.clone() {
            if asn < tx_slot {
                // Check for clear slots
                // TODO: this needs to be called multiple times in a slot (or offset into the slot to see the RX) rather than once per ASN as is currently guarded in `tick`
                let rssi = self.base.rssi(now_ms)?;
                if rssi > self.config.channel_clear_threshold {

                    // If we're not clear, try again
                    debug!("CCA fail at ASN: {} (rssi: {})", asn, rssi);

                    self.csma_state = CsmaState::Pending{
                        packet: packet.clone(),
                        tx_slot: 0,
                        retries: retries + 1,
                    };
                }

            } else if asn == tx_slot {
                // Prepare packet and transmit
                let mut buff = [0u8; 255];
                let n = packet.encode(&mut buff, WriteFooter::No);

                self.base.transmit(now_ms, &buff[..n])?;

                debug!("CSMA TX at {} ms", now_ms);

                // Update CSMA state and packet buffer
                self.csma_state = CsmaState::None;

                if !packet.header.ack_request {
                    let _ = self.tx_buff.dequeue();
                } else {
                    // TODO: arm ACK RX?
                }

            } else if tx_slot != 0 && asn > tx_slot {
                warn!("CSMA TX slot miss");
                self.stats.deadline_miss_tx = self.stats.deadline_miss_tx.saturating_add(1);

                self.csma_state = CsmaState::Pending{
                    packet: packet.clone(),
                    tx_slot: 0,
                    retries: retries + 1,
                };
            }
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

        trace!("Received {} byte {:?} packet", p.payload().len(), p.header.frame_type);

        // Filter by PAN ID
        let pan_id = p.pan_id();
        if pan_id != PanId::broadcast() {
            match &self.assoc_state {
                AssocState::Associated(id) if pan_id != *id => {
                    debug!("Pan ID mismatch, dropped packet {} for {:?}", p.header.seq, pan_id);  
                    return Ok(())
                },
                _ => (),
            }
        }

        // Filter by address
        match (p.header.destination, self.short_addr) {
            // Accept messages to broadcast short address
            (Address::Short(_, short), _) if short == ShortAddress::broadcast() => (),
            // Accept messages to our short address
            (Address::Short(_, short), Some(addr)) if short == addr => (),
            // Accept messages to our extended address
            (Address::Extended(_, ext), _) if ext == self.address => (),
            _ => {
                debug!("Address mismatch, dropped packet {} for {:?}", p.header.seq, p.header.destination);  
                return Ok(())
            },
        };

        // Arm ACK response if required
        if p.header.ack_request {
            // Build ACK payload

            let ack = Packet::ack(&p);
            self.ack_state = AckState::Pending{
                tx_time: now + self.config.ack_delay, 
                packet: ack,
            };

            debug!("Scheduled ACK for packet {} from {:?} for {} ms", p.header.seq, p.header.source, now + self.config.ack_delay);
        }

        // Handle received packets
        match p.content {
            FrameContent::Beacon(_b) => {
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
                    self.beacon_miss_count = 0;

                    debug!("Received beacon at {} ms (set offset to {} ms)",
                        now, self.sync_offset);

                // If we're synced use this to evaluate drift and correct _if_ it's from
                //our parent
                } else if let SyncState::Synced(parent) = self.sync_state {
                    if p.header.source != parent {
                        debug!("Disgarding sync from non-parent: {:?}", p.header.source);

                    } else {
                        // Compute offset from expected time
                        // This is improved by TSCH EBs / ASNs huh?
                        // TODO: what happens if we're > one slot out of sync
                        let delta = (now as i64 - self.next_beacon as i64) as i64
                                % self.config.superframe_duration() as i64;

                        trace!("current offset: {} delta: {}", self.sync_offset, delta);
                        
                        // Update stack synchronization offset
                        // TODO: improve this to a piecewise / averaging offset correction
                        if delta.abs() > self.config.superframe_duration() as i64 / 10 {
                            // Ignore huge corrections (ie. one slot out of time)
                        } else if delta < 0 {
                            self.sync_offset -= delta.abs() as u64 / 2;
                        } else {
                            self.sync_offset += delta.abs() as u64 / 2;
                        }
                        
                        debug!("Received new beacon at {} ms (expected at {} ms, error: {} ms, updated offset to {} ms)",
                        now, self.next_beacon, delta, self.sync_offset);

                        // Set new beacon time
                        // TODO: really this should happen in tick rather than here?
                        self.next_beacon = now + self.config.superframe_duration() as u64;
                        self.beacon_miss_count = 0;
                        debug!("Arm next beacon RX at {} ms", self.next_beacon);
                    }
                }

                // TODO: apply beacon info to config?
                // How to do this in a transient way? maybe hold separately and merge?

            },
            FrameContent::Command(c) => {

                match c {
                    Command::AssociationRequest(req) => {
                        debug!("Association request from: {:?} (cap: {:?}", p.header.source, req);

                        // TODO: check whether to allow association?

                        // TODO: how do we _reasonably_ assign short addresses here?
                        // For global uniqueness we either need to know all of em or
                        // go back to the pan_coordinator for assignment?
                        // For now, use no-assign short addr
                        let assoc_addr = ShortAddress(0xfffe);
                        let assoc_status = AssociationStatus::Successful;

                        // Build response
                        let assoc_cmd = Command::AssociationResponse(assoc_addr, assoc_status);
                        let assoc_resp = Packet::command(p.header.source, self.addr(), self.seq(), assoc_cmd);

                        if let Err(_) = self.tx_buff.enqueue((TxState::default(), assoc_resp)) {
                            error!("Error adding associate request to tx buffer");
                        }

                    },
                    Command::AssociationResponse(_assoc_addr, assoc_state) => {
                        // Only handle expected associations
                        match self.assoc_state {
                            AssocState::Unassociated | AssocState::Associated(_) => return Ok(()),
                            AssocState::Pending(addr, _) if addr != p.header.source => {
                                warn!("Associate response from unexpected peer {:?}", p.header.source);
                                return Ok(())
                            }
                            _ => (),
                        }

                        if assoc_state == AssociationStatus::Successful {

                            let pan_id = p.header.source.pan_id().unwrap();
                            info!("Associated with PAN: {}!", pan_id.0);

                            // TODO: apply short address if received
                            
                            // TODO: extract pan ID to support compression?
                            self.assoc_state = AssocState::Associated(pan_id);
                        } else {
                            warn!("Association failed with status: {:?}", assoc_state);

                            // TODO: add back-off or reset sync on failure?
                            self.assoc_state = AssocState::Unassociated;
                        }
                    }
                    _ => {
                        info!("RX unhandled command: {:?}", c);
                    },
                }

            },
            FrameContent::Acknowledgement => {
                match self.tx_buff.peek() {
                    Some((_s, t)) if p.is_ack_for(t) => {
                        debug!("ACK rx for packet: {}!", p.header.seq);

                        // Remove from TX buffer
                        // TODO: signal success to higher level?
                        let _ = self.tx_buff.dequeue();
                    }
                    Some((_s, _t)) => {
                        warn!("ACK sequence mismatch");
                    },
                    None => {
                        warn!("ACK with no pending operation");
                    }
                }
            },
            FrameContent::Data => {
                debug!("Received {} bytes of data from {:?}", p.payload().len(), p.header.source);

                let i = RxInfo{
                    source: p.header.source,
                    rssi: rx.rssi,
                };

                // Enqueue in RX buffer
                if let Err(_e) = self.rx_buff.enqueue((i, p)) {
                    error!("Error adding packet to RX queue");
                }
            },
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use ieee802154::mac::*;
    use radio::{BasicInfo, mock::*};
    
    use crate::timer::mock::MockTimer;
    use super::*;

    #[test]
    fn beacon_tx() {
        let _ = simplelog::SimpleLogger::init(log::LevelFilter::Trace, simplelog::Config::default());
        
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
        let seq = 0;

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
