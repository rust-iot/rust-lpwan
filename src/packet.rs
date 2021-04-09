
use log::{error};

use ieee802154::mac::*;

use heapless::{Vec, consts::U256};

// TODO: fix or remove this?
pub const MAX_PAYLOAD_LEN: usize = 256;

/// Packet object represents an IEEE 802.15.4 object with owned storage.
/// 
/// Based on https://docs.rs/ieee802154/0.3.0/ieee802154/mac/frame/struct.Frame.html
#[derive(Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Packet {
    pub header: Header,

    pub content: FrameContent,

    payload: Vec<u8, U256>,

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

    pub fn data(dest: Address, source: Address, seq: u8, data: &[u8]) -> Packet {
        let mut payload = Vec::new();

        if let Err(e) = payload.extend_from_slice(data) {
            panic!("Error encoding payload: {:?}", e);
        };
        
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
            payload,
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

    // Check whether this packet is an ack for the provided packet
    pub fn is_ack_for(&self, original: &Packet) -> bool {
        self.header.frame_type == FrameType::Acknowledgement &&
        self.header.source == original.header.destination &&
        self.header.destination == original.header.source && 
        self.header.seq == original.header.seq && 
        self.content == FrameContent::Acknowledgement
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
