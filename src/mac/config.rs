

use ieee802154::mac::*;

/// Configuration for the basic MAC
#[derive(Clone, PartialEq, Debug)]
pub struct CoreConfig {
    /// RSSI threshold for a channel to be determined to be clear
    pub channel_clear_threshold: i16,
    
    /// Timeout for message ACK (if required) in milliseconds
    pub ack_timeout_ms: u32,

    /// Number of retries for acknowleged messages
    pub max_retries: u16,

    pub rx_has_footer: bool,
    //pub tx_write_footer: WriteFooter,

    /// Enable software-level ACK transmission
    pub send_acks: bool,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {           
            channel_clear_threshold: -90,
            ack_timeout_ms: 10,
            max_retries: 5,

            rx_has_footer: false,
            //tx_write_footer: WriteFooter::No,

            send_acks: true,
        }
    }
}

pub struct AddressConfig {
    pub pan_id: PanId,

    pub short_address: Option<ShortAddress>,

    pub extended_address: Option<ExtendedAddress>,
}

impl AddressConfig {
    pub fn new(pan_id: u16, extended_address: u64) -> Self {
        Self{
            pan_id: PanId(pan_id),
            short_address: None,
            extended_address: Some(ExtendedAddress(extended_address)),
        }
    }

    pub fn get(&self) -> Address {
        if let Some(s) = self.short_address {
            return Address::Short(self.pan_id, s)
        }
        if let Some(e) = self.extended_address {
            return Address::Extended(self.pan_id, e)
        }
        
        Address::None
    }
}

/// Configuration for beaconing mode
pub struct BeaconConfig {
    /// Enable beacon frame transmission
    pub enabled: bool,
    /// Beacon period in microseconds (ie. superframe period)
    pub period_us: u32,
    /// Length of each slot in microseconds
    pub slot_time_us: u32,
    /// number of slots per superframe
    pub slots_per_round: u16,
}

impl Default for BeaconConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            period_us: 10 * 1000,
            slots_per_round: 10,
            slot_time_us: 10 * 1000,
        }
    }
}
