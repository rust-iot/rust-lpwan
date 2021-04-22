//! 6LowPAN Example Application
//
// https://github.com/rust-iot/rust-lpwan
// Copyright 2021 Ryan Kurte

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use log::{debug, info, error};

use structopt::StructOpt;

use embedded_hal::blocking::delay::DelayMs;
use driver_pal::hal::{HalInst, HalDelay, DeviceConfig};

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

impl MacTimer for SystemTimer {
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
    let mut rf_config = Sx128xConfig::gfsk();
    if let Modem::Gfsk(gfsk) = &mut rf_config.modem {
        gfsk.patch_preamble = false;
        gfsk.crc_mode = radio_sx128x::device::common::GfskFlrcCrcModes::RADIO_CRC_2_BYTES;
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
    let address = ExtendedAddress(rand::random::<u64>() % 1000);
    let mac_config = mac_802154::Config {
        pan_coordinator: opts.coordinator,
        ..Default::default()
    };

    debug!("Initialising MAC");

    let timer = SystemTimer::new();
    let mac = match mac_802154::Mac::new(address, mac_config, radio, timer.clone()) {
        Ok(m) => m,
        Err(e) => {
            return Err(anyhow::anyhow!("Error initalising MAC: {:?}", e));
        }
    };

    debug!("Initialising 6lo");
    let sixlo_cfg = SixLoConfig{
        ..Default::default()
    };
    let mut sixlo = SixLo::<_, _, 127>::new(mac, MacAddress::Extended(PanId(1), address), sixlo_cfg);


    debug!("Starting loop");

    let mut last_tx = timer.ticks_ms();

    while running.load(Ordering::SeqCst) {
        let now = timer.ticks_ms();
        
        // Update the mac
        match sixlo.tick(now) {
            Ok(_) => (),
            Err(e) => {
                error!("MAC tick error: {:?}", e);
            }
        }

        // Check for RX'd packets
        let mut buff = [0u8; 256];
        match sixlo.receive(now, &mut buff) {
            Ok(Some((n, _h))) => {
                info!("Received data: {:02x?}", &buff[..n]);
            },
            Err(e) => {
                error!("MAC RX error: {:?}", e)
            },
            _ => (),
        }

        // Periodic transmit
        if opts.coordinator && now > last_tx + 10_000 {
            let data = [now as u8; 200];

            info!("TX {:02x} ({} bytes) at {} ms", data[0], data.len(), now);

            if let Err(e) = sixlo.transmit(now, MacAddress::broadcast(&AddressMode::Short), &data) {
                error!("MAC TX error: {:?}", e);
            }

            last_tx = now;
        }

        // TODO: rx / tx packets

        // TODO: wait a wee while for the next tick
        HalDelay{}.try_delay_ms(1).unwrap();
    }

    Ok(())
}
