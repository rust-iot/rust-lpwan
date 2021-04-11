

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use std::collections::{HashMap, hash_map::Entry};
use std::convert::TryFrom;

use log::{debug, info, warn, error};

use structopt::StructOpt;
use humantime::Duration;

use embedded_hal::blocking::delay::DelayMs;
use driver_pal::hal::{HalInst, HalDelay, DeviceConfig};

use radio::blocking::*;
use radio_sx128x::prelude::*;
use radio_sx128x::{Config as Sx128xConfig};

use lpwan::prelude::*;


#[derive(Debug, StructOpt)]
struct Options {

    #[structopt(flatten)]
    pub spi_config: DeviceConfig,

    #[structopt(long)]
    /// Run as a PAN coordinator
    pub coordinator: bool,

    #[structopt(long, default_value="100")]
    /// Set PAN ID
    pub pan_id: u16,

    #[structopt(long, default_value = "info")]
    /// Configure radio log level
    pub log_level: simplelog::LevelFilter,
}

#[derive(Clone, Debug)]
pub struct SystemTimer {
    start: Instant,
}

impl SystemTimer {
    fn new() -> Self {
        Self {
            start: Instant::now()
        }
    }
}

impl Timer for SystemTimer {
    fn ticks_ms(&self) -> u64 {
        Instant::now().duration_since(self.start).as_millis() as u64
    }

    fn ticks_us(&self) -> u64 {
        Instant::now().duration_since(self.start).as_micros() as u64
    }
}


fn main() -> anyhow::Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // Bind exit handler
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

    // Load options
    let opts = Options::from_args();

    // Initialise logging
    let log_cfg = simplelog::ConfigBuilder::new()
        .add_filter_ignore_str("radio_sx128x")
        .add_filter_ignore_str("driver_cp2130")
        .build();
    let _ = simplelog::SimpleLogger::init(opts.log_level, log_cfg);

    info!("Starting lpwan-sx128x");

    debug!("Connecting to HAL");
    let HalInst{base: _, spi, pins} = match HalInst::load(&opts.spi_config) {
        Ok(v) => v,
        Err(e) => {
            return Err(anyhow::anyhow!("HAL error: {:?}", e));
        }
    };

    debug!("Initialising Radio");
    let mut rf_config = Sx128xConfig::lora();
    if let Modem::Gfsk(gfsk) = &mut rf_config.modem {
        gfsk.patch_preamble = false;
        //gfsk.crc_mode = radio_sx128x::device::common::GfskFlrcCrcModes::RADIO_CRC_2_BYTES;
    }

    let mut radio = match Sx128x::spi(spi, pins.cs, pins.busy, pins.ready, pins.reset, HalDelay{}, &rf_config) {
        Ok(v) => v,
        Err(e) => {
            return Err(anyhow::anyhow!("Radio init error: {:?}", e));
        }
    };

    if let Modem::Gfsk(_gfsk) = &mut rf_config.modem {
        radio.set_syncword(1, &[0x11, 0x22, 0x33, 0x44, 0x55]).unwrap();
    }

    // Initialise network stack
    let address = ExtendedAddress(10);
    let mut mac_config = MacConfig {
        pan_coordinator: opts.coordinator,
        ..Default::default()
    };

    debug!("Initialising MAC");

    let timer = SystemTimer::new();
    let mut mac = match Mac::new(address, mac_config, radio, timer) {
        Ok(m) => m,
        Err(e) => {
            return Err(anyhow::anyhow!("Error initalising MAC: {:?}", e));
        }
    };


    debug!("Starting loop");

    while running.load(Ordering::SeqCst) {
        
        // Update the mac
        match mac.tick() {
            Ok(_) => (),
            Err(e) => {
                error!("MAC error: {:?}", e);
            }
        }

        // TODO: rx / tx packets

        // TODO: wait a wee while for the next tick
        HalDelay{}.try_delay_ms(1).unwrap();
    }

    Ok(())
}
