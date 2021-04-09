

use core::{fmt::Debug, marker::PhantomData};


use log::{debug, info, warn};

use ieee802154::mac::ExtendedAddress;

use crate::{Radio, timer::Timer, base::Base, error::CoreError};


#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub coordinator: bool,

    /// Base superframe duration in ms
    pub base_superframe_duration: u32,

    /// Mac beacon order, sets superframe length
    /// 
    /// beacon period = base_superframe_duration * 2^mac_beacon_order, 
    /// thus a value of 0 sets the superframe length to base_superframe_duration
    /// Valid values are 0 < v < 15, a value of 15 disables sending beacon frames
    pub mac_beacon_order: u32,

    /// Mac superframe order (ie. how much of that superframe is active)
    ///
    /// SD = base_superframe_duration * 2^mac_superframe_order,
    /// thus for a mac_beacon_order of 1, a mac_superframe_order of 0 would
    /// be of 2*base_superframe_duration length with a base_superframe_duration active period.
    /// Valid values are 0 < v < 15, a value of 15 disables the whole superframe
    pub mac_superframe_order: u32,

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
            coordinator: false,

            base_superframe_duration: 1000,
            base_slot_duration: 100,

            mac_beacon_order: 1,
            mac_superframe_order: 0,
            mac_deadline: 2,

            max_retries: 5,
            min_be: 1,
            max_be: 5,
        }
    }
}

impl Config {
    pub fn superframe_duration(&self) -> u32 {
        (self.base_superframe_duration * 2_u32.pow(self.mac_beacon_order)) as u32
    }

    pub fn calculate_sfn(&self, now: u64, offset: u64) -> u64 {
        (now + offset) / self.superframe_duration() as u64
    }

    pub fn calculate_asn(&self, now: u64, offset: u64) -> u64 {
        (now + offset) / self.superframe_duration() as u64 / self.base_slot_duration as u64
    }
}



#[derive(Debug, Clone, PartialEq)]
pub struct Mac<R, I, E, T> {
    pub address: ExtendedAddress,
    config: Config,
    base: Base<R, I, E>,
    timer: T,

    sync_offset: u64,
    last_asn: u64,
    next_beacon: u64,
}

impl <R, I, E, T> Mac<R, I, E, T> 
where
    R: Radio<I, E>,
    I: radio::ReceiveInfo + Default + Debug,
    E: Clone + Debug,
    T: Timer,
{
    pub fn new(address: ExtendedAddress, config: Config, radio: R, timer: T) -> Result<Self, CoreError<E>> {
        let mut s = Self {
            address,
            config,

            base: Base::new(radio)?,
            timer,
            
            sync_offset: 0,
            last_asn: 0,
            next_beacon: 0,
        };

        let now = s.timer.ticks_ms();
        s.sync_offset = now;

        debug!("Setup MAC at {} ms", now);

        if s.config.coordinator && s.config.mac_beacon_order < 15 {
            s.next_beacon = now + s.config.superframe_duration() as u64;
            debug!("Setup next beacon for {} ms", s.next_beacon);
        }

        Ok(s)
    }

    fn tick(&mut self) -> Result<(), CoreError<E>> {

        let now_ms = self.timer.ticks_ms();

        let sfn = self.config.calculate_sfn(now_ms, self.sync_offset);
        let asn = self.config.calculate_asn(now_ms, self.sync_offset);

        info!("Tick at {} ms with ASN: {} (SFN: {})", now_ms, asn, sfn);

        // Update radio interface
        self.base.tick()?;

        // Broadcast beacon at configured intervals
        // TODO: add time compensation to prepare radio / packet for TX
        if self.next_beacon != 0 && self.next_beacon <= now_ms {
            // Check for beacon schedule misses
            if (self.next_beacon + self.config.mac_deadline as u64) < now_ms {
                warn!("MAC deadline exceeded");
            }

            debug!("Broadcasting EB at ASN: {}", asn);

            // Re-arm beacon
            if self.config.coordinator {
                self.next_beacon += self.config.superframe_duration() as u64;
            }
        }



        unimplemented!()

    }
}

