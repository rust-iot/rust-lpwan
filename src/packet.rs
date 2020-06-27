
use ieee802154::mac::*;

pub const MAX_PAYLOAD_LEN: usize = 128;

/// Packet object represents an IEEE 802.15.4 object with owned storage
pub struct Packet {
    pub header: Header,

    pub content: FrameContent,

    payload: [u8; MAX_PAYLOAD_LEN],
    payload_len: usize,

    pub footer: [u8; 2],
}

impl Packet {
    // Based on https://docs.rs/ieee802154/0.3.0/ieee802154/mac/frame/struct.Frame.html#method.encode
    pub fn encode(&self, buf: &mut [u8], write_footer: WriteFooter) -> usize {
        let mut len = 0;

        // Write header
        len += self.header.encode(&mut buf[len..]);

        // Write content
        len += self.content.encode(&mut buf[len..]);

        // Write payload
        buf[len .. len+self.payload_len].copy_from_slice(&self.payload);

        len += self.payload_len;

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
        let body = &buf[header_len..remaining];

        // Decode the FrameContent
        let (content, used) = FrameContent::decode(body, &header)?;
        remaining -= used;

        // Copy out the payload
        let mut payload = [0u8; MAX_PAYLOAD_LEN];
        (&mut payload[..remaining]).copy_from_slice(&body[used..]);

        Ok(Packet {
            header,
            content,
            payload,
            payload_len: remaining,
            footer,
        })
    }

    pub fn get_payload(&self) -> &[u8] {
        &self.payload[..self.payload_len]
    }

    pub fn set_payload(&mut self, body: &[u8]) -> Result<(), ()> {
        if body.len() > MAX_PAYLOAD_LEN {
            return Err(())
        }

        (&mut self.payload[..body.len()]).copy_from_slice(body);
        self.payload_len = body.len();

        Ok(())
    }
}
