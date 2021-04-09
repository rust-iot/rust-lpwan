
use crate::packet::Packet;
use ieee802154::mac::DecodeError;

/// Basic MAC errors
#[derive(Debug, Clone, PartialEq)]
pub enum CoreError<E> {
    /// Buffer full
    BufferFull(Packet),

    /// Transmission buffer full
    TransmitPending,

    /// Transmission failed
    TransmitFailed(Packet),

    /// Decoding error
    DecodeError(DecodeError),

    /// Wrapper for unhandled / underlying radio errors
    Radio(E),

    Timeout,

    Busy,
}

