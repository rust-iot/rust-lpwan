# Rust Low Power Wireless Area Network (LPWAN) Network Stack

An (extremely early, experimental, not yet for real-use) LPWAN network stack in rust.
This is intended to provide a simple / testable / composable stack for IoT devices, compatible with common LPWAN network technologies, and is designed for use with [radio-hal](https://github.com/rust-iot/radio-hal) supported devices.


## Status

[![GitHub tag](https://img.shields.io/github/tag/rust-iot/rust-lpwan.svg)](https://github.com/rust-iot/rust-lpwan)
[![Build Status](https://travis-ci.com/rust-iot/rust-lpwan.svg?token=s4CML2iJ2hd54vvqz5FP&branch=master)](https://travis-ci.com/rust-iot/rust-lpwan)
[![Crates.io](https://img.shields.io/crates/v/lpwan.svg)](https://crates.io/crates/lpwan)
[![Docs.rs](https://docs.rs/lpwan/badge.svg)](https://docs.rs/lpwan)

[Open Issues](https://github.com/rust-iot/rust-lpwan/issues)

## Features

- [ ] 802.15.4 - [802.15.4-2015](https://ieeexplore.ieee.org/document/7460875)
  - [ ] CSMA MAC
  - [ ] TiSCH MAC - [ietf-6tisch-minimal-](https://tools.ietf.org/html/draft-ietf-6tisch-minimal-21#section-8.4.2.2.3)
- [ ] LoRaWAN
  - [ ] MAC
  - [ ] ..?
- [ ] NDP - [rfc4861](https://tools.ietf.org/html/rfc4861)
- [ ] RPL - [rfc6550](https://tools.ietf.org/html/rfc6550)
- [ ] 6LowPan - [rfc4944](https://tools.ietf.org/html/rfc4944), [rfc6282](https://tools.ietf.org/html/rfc6282)
- [ ] Thread

