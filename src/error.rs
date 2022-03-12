//! LPWAN Error Types
//
// https://github.com/rust-iot/rust-lpwan
// Copyright 2021 Ryan Kurte

use ieee802154::mac::DecodeError;

/// Basic MAC errors
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature="defmt", derive(defmt::Format))]
pub enum CoreError<E> {
    /// Buffer full
    BufferFull,

    /// Transmission buffer full
    TransmitPending,

    /// Transmission failed
    TransmitFailed,

    /// Decoding error
    DecodeError(DecodeError),

    /// Wrapper for unhandled / underlying radio errors
    Radio(E),

    Timeout,

    Busy,
}

