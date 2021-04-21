

use ieee802154::mac::{PanId};
use ieee802154::mac::beacon::{
    BeaconOrder,
    SuperframeOrder,
    SuperframeSpecification,
};

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

    /// Number of missed beacons before desync
    pub max_beacon_misses: u32,

    /// Timeout for association requests
    pub assoc_timeout: u64,

    /// Battery life extension flag, allows 0 slot minimum CSMA backoff
    pub battery_life_extension: bool,

    /// Maximum number of retries
    pub max_retries: u8,

    /// Delay between packet RX and ACK
    pub ack_delay: u64,

    /// Minimum backoff exponent
    pub min_be: u8,
    /// Maximum backoff exponent
    pub max_be: u8,
    /// RSSI threshold for a channel to be determined to be clear
    pub channel_clear_threshold: i16,
    /// Maximum number of backoffs
    pub csma_max_backoffs: u8,

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
            mac_deadline: 10,

            max_beacon_misses: 10,
            assoc_timeout: 10*1000,
            battery_life_extension: true,

            max_retries: 5,
            ack_delay: 50,

            min_be: 2,
            max_be: 5,
            csma_max_backoffs: 3,
            channel_clear_threshold: -50,
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

    pub fn slots_per_slotframe(&self) -> u64 {
        (self.base_superframe_duration / self.base_slot_duration) as u64
    }

    pub fn calculate_sfn(&self, now: u64, offset: u64) -> u64 {
        (now + offset) / self.superframe_duration() as u64
    }

    pub fn calculate_asn(&self, now: u64, offset: u64) -> u64 {
        (now + offset) / self.base_slot_duration as u64
    }

    pub fn calculate_rsn(&self, now: u64, offset: u64) -> u64 {
        // TODO: not _sure_ this is correct, slotframe/superframe needs updating to 2015
        self.calculate_asn(now, offset) % self.slots_per_slotframe()
    }
}
