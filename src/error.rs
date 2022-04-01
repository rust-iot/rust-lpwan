//! LPWAN Error Types
//
// https://github.com/rust-iot/rust-lpwan
// Copyright 2021 Ryan Kurte

use ieee802154::mac::DecodeError;

use crate::MacError;

/// Basic MAC errors
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

impl<E> MacError for CoreError<E> {
    fn queue_full(&self) -> bool {
        match self {
            Self::BufferFull => true,
            _ => false,
        }
    }
}
