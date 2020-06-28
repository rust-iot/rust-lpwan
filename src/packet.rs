
use ieee802154::mac::*;

use heapless::{Vec, consts::U128};

pub const MAX_PAYLOAD_LEN: usize = 128;

/// Packet object represents an IEEE 802.15.4 object with owned storage.
/// 
/// Based on https://docs.rs/ieee802154/0.3.0/ieee802154/mac/frame/struct.Frame.html
#[derive(Clone, Debug)]
pub struct Packet {
    pub header: Header,

    pub content: FrameContent,

    payload: Vec<u8, U128>,

    pub footer: [u8; 2],
}

impl PartialEq for Packet {
    fn eq(&self, o: &Self) -> bool {
        self.header == o.header &&
        self.content == o.content && 
        self.payload() == o.payload() &&
        self.footer == o.footer
    }
}

impl Packet {

    pub fn data<D: AsRef<[u8]>>(dest: Address, source: Address, seq: u8, data: D) -> Packet {
        let d = data.as_ref();

        Packet {
            header: Header {
                frame_type: FrameType::Data,
                frame_pending: false,
                security: Security::None,
                ack_request: false,
                pan_id_compress: false,
                version: FrameVersion::Ieee802154_2006,
                destination: dest,
                source: source,
                seq: seq,
            },
            content: FrameContent::Data,
            payload: Vec::from_slice(d).unwrap(),
            footer: [0u8; 2],
        }
    }

    // Generate an ACK for the provided packet
    pub fn ack(request: &Packet) -> Packet {
        Packet {
            header: Header {
                frame_type: FrameType::Acknowledgement,
                frame_pending: false,
                security: Security::None,
                ack_request: false,
                pan_id_compress: false,
                version: FrameVersion::Ieee802154_2006,
                destination: request.header.source,
                source: request.header.destination,
                seq: request.header.seq,
            },
            content: FrameContent::Acknowledgement,
            payload: Vec::new(),
            footer: [0u8; 2],
        }
    }

    // Check whether a received packet is an ack for this packet
    pub fn is_ack(&self, maybe_ack: &Packet) -> bool {
        maybe_ack.header.frame_type == FrameType::Acknowledgement &&
        maybe_ack.header.source == self.header.destination &&
        maybe_ack.header.destination == self.header.source && 
        maybe_ack.header.seq == self.header.seq && 
        maybe_ack.content == FrameContent::Acknowledgement
    }

    // Based on https://docs.rs/ieee802154/0.3.0/ieee802154/mac/frame/struct.Frame.html#method.encode
    pub fn encode(&self, buf: &mut [u8], write_footer: WriteFooter) -> usize {
        let mut len = 0;

        // Write header
        len += self.header.encode(&mut buf[len..]);

        // Write content
        len += self.content.encode(&mut buf[len..]);

        // Write payload
        buf[len .. len+self.payload.len()].copy_from_slice(&self.payload);

        len += self.payload.len();

        // Write footer
        match write_footer {
            WriteFooter::No => (),
        }
        len
    }

    // Based on https://docs.rs/ieee802154/0.3.0/ieee802154/mac/frame/struct.Frame.html#method.decode
    pub fn decode(buf: &[u8], contains_footer: bool) -> Result<Self, DecodeError> {
        let mut remaining = buf.len();

        // First decode header
        let (header, header_len) = Header::decode(buf)?;
        remaining -= header_len;

        // If there's a footer, decode this
        let mut footer = [0; 2];
        if contains_footer {
            if remaining < 2 {
                return Err(DecodeError::NotEnoughBytes);
            }

            let footer_pos = buf.len() - 2;
            footer.copy_from_slice(&buf[footer_pos..]);

            remaining -= 2;
        }

        // Fetch the body subslice
        let body = &buf[header_len..header_len+remaining];

        // Decode the FrameContent
        let (content, used) = FrameContent::decode(body, &header)?;
        remaining -= used;

        // Copy out the payload
        let payload = Vec::from_slice(&body[used..]).map_err(|_e| DecodeError::NotEnoughBytes)?;

        Ok(Packet {
            header,
            content,
            payload,
            footer,
        })
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn set_payload(&mut self, body: &[u8]) -> Result<(), ()> {
        self.payload = Vec::from_slice(body)?;

        Ok(())
    }
}
