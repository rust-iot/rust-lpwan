//! 6LoWPAN/IPv6 Headers
//
// https://github.com/rust-iot/rust-lpwan
// Copyright 2021 Ryan Kurte

use byteorder::{ByteOrder, LittleEndian};

use ieee802154::mac::{Address, DecodeError, ExtendedAddress, PanId, ShortAddress};


// https://tools.ietf.org/html/rfc4944#page-3

#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Header {
    pub hc1: Option<Hc1Header>,
    pub mesh: Option<MeshHeader>,
    pub bcast: Option<BroadcastHeader>,
    pub frag: Option<FragHeader>,
}

impl Default for Header {
    fn default() -> Self {
        Self {
            hc1: None,
            mesh: None,
            bcast: None,
            frag: None,
        }
    }
}

impl Header {
    pub fn merge(&mut self, h: &Header) {
        match (self.mesh.is_none(), &h.mesh) {
            (true, Some(h)) => self.mesh = Some(h.clone()),
            _ => (),
        }

        match (self.bcast.is_none(), &h.bcast) {
            (true, Some(h)) => self.bcast = Some(h.clone()),
            _ => (),
        }

        match (self.frag.is_none(), &h.frag) {
            (true, Some(h)) => self.frag = Some(h.clone()),
            _ => (),
        }

        match (self.hc1.is_none(), &h.hc1) {
            (true, Some(h)) => self.hc1 = Some(h.clone()),
            _ => (),
        }
    }

    pub fn decode(buff: &[u8]) -> Result<(Self, usize), DecodeError> {
        let mut offset = 0;

        // Skip non-lowpan packets
        if buff[0] & HEADER_TYPE_MASK == HeaderType::Nalp as u8 {
            return Ok((Header::default(), 0));
        }

        // Parse out mesh headers
        let mesh = if buff[offset] & HEADER_TYPE_MASK == HeaderType::Mesh as u8 {
            let (m, n) = MeshHeader::decode(&buff[offset..])?;
            offset += n;
            Some(m)
        } else {
            None
        };
        
        // TODO: deocde BC0 broadcast header
        let bcast = None;

        // Parse fragmentation header
        let frag = if buff[offset] & HEADER_TYPE_MASK == HeaderType::Frag as u8 {
            let (m, n) = FragHeader::decode(&buff[offset..])?;
            offset += n;
            Some(m)
        } else {
            None
        };

        // Parse out HC1
        // Disabled due to parsing error, check the type mask better...
        #[cfg(nope)]
        let hc1 = if buff[offset] & HEADER_TYPE_MASK == HeaderType::Lowpan as u8 {
            let (m, n) = Hc1Header::decode(&buff[offset..])?;
            offset += n;
            Some(m)
        } else {
            None
        };

        let hc1 = None;

        // TODO: parse out IPv6 uncompressed header

        Ok(( Self{ hc1, mesh, bcast, frag }, offset ))
    }

    pub fn encode(&self, buff: &mut[u8]) -> usize {
        let mut offset = 0;

        if let Some(mesh) = &self.mesh {
            offset += mesh.encode(&mut buff[offset..]);
        }

        if let Some(_bcast) = &self.bcast {
            // TODO: encode BC0 broadcast header
        }

        if let Some(frag) = &self.frag {
            offset += frag.encode(&mut buff[offset..]);
        }

        if let Some(hc1) = &self.hc1 {
            offset += hc1.encode(&mut buff[offset..]);
        }

        offset
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum HeaderType {
    /// Not a LoWPAN Frame (discard packet)
    Nalp = 0b0000_0000,
    /// LoWPAN Headers
    Lowpan = 0b0000_0001,
    /// Mesh Headers
    Mesh = 0b0000_0010,
    /// Fragmentation headers
    Frag = 0b0000_0011,
}

pub const HEADER_TYPE_MASK: u8 = 0b0000_0011;
pub const HEADER_DISPATCH_MASK: u8 = 0b1111_1100;

/// Dispatch types per [RFC4449 Section 5.1](https://tools.ietf.org/html/rfc4944#section-5.1)
// TODO: shit are these all backwards?!
#[derive(Copy, Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DispatchBits {
    /// Not a LoWPAN Frame (discard packet)
    Nalp = 0b0000_0000,
    /// Uncompressed IPv6 header
    Ipv6 = 0b0100_0001,
    /// LOWPAN_HC1 compressed IPV6 header
    Hc1 =  0b0100_0010,
    /// LOWPAN_BC0 broadcast
    Bc0 = 0b0101_0000,
    /// ESC(ape), additional dispatch byte follows
    Esc = 0b0111_1111,
    /// Mesh header (0b10xx_xxxx)
    Mesh = 0b1000_0000,
    /// Fragmentation header (first, 0b1100_0xxx)
    Frag1 = 0b1100_0000,
    /// Fragmentation header (N, 0b1110_0xxx)
    FragN = 0b1110_0000
}

/// IPHC Header
/// https://tools.ietf.org/html/draft-ietf-6lowpan-hc-15
#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct IphcHeader {
    pub flags_0: IphcFlags0,
    pub flags_1: IphcFlags1,
}

bitflags::bitflags!{
    /// IPHC flags byte 1
    /// https://tools.ietf.org/html/draft-ietf-6lowpan-hc-15#section-3.1.1
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub struct IphcFlags0: u8 {
        /// Traffic Control / Flow Label - ECN + DSCP + 4-bit Pad + Flow Label (4 bytes)
        const TCFL_FULL     = 0b0000_0000;
        /// Traffic Control / Flow Label - ECN + 2-bit Pad + Flow Label (3 bytes), DSCP is elided
        const TCFL_NO_DSCP  = 0b0000_1000;
        /// Traffic Control / Flow Label - ECN + DSCP (1 byte), Flow Label is elided
        const TCFL_NO_FL    = 0b0001_0000;
        /// Traffic Control / Flow Label - Traffic Class and Flow Label are elided.
        const TCFL_ELIDE    = 0b0001_1000;

        /// Next header compressed and encoded via LOWPAN_NHC.
        /// otherwise full 8 header bits are inline
        const NEXT_HDR_COMPRESS = 0b0010_0000;

        /// Hop limit compressed with limit of 1
        const HOP_LIMIT1        = 0b0100_0000;
        /// Hop limit compressed with limit of 64
        const HOP_LIMIT64       = 0b1000_0000;
        /// Hop limit compressed with limit of 255
        const HOP_LIMIT255      = 0b1100_0000;

        /// Base bits (from dispatch)
        const BASE = 0b0000_0110;
    }
}

bitflags::bitflags!{
    /// IPHC flags byte 2
    /// https://tools.ietf.org/html/draft-ietf-6lowpan-hc-15#section-3.1.1
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub struct IphcFlags1: u8 {
        /// Additional 8-bit Context Identifier Extension field immediately follows the DAM field.
        const CID_EXT     = 0b0000_0001;

        /// Source Address Compression (SAC) uses stateful, context-based compression.
        const SAC_STATEFULL = 0b0000_0010;

        /// if SAC=0, 128 bit source address, carred inline
        /// if SAC=1, UNSPECIFIED address `::`
        const SAM_128BIT_UNSPEC = 0b0000_0000;
        /// if SAC=0, 64 bit source address, first 64-bits of the address are elided.
        /// if SAC=1, 64-bit source address, derived from context and 64 inline bits
        const SAM_64BIT = 0b0000_0100;
        /// if SAC=0, 16 bit source address, first 112-bits of the address are elided.
        /// if SAC=1, 16-bit source address, derived from context and 16-bits inline
        const SAM_16BIT = 0b0000_1000;
        /// if SAC=0, 0 bit source address, computed from encapsulating header
        /// if SAC=0, 0 bit source address, derived from context and encapsulating header
        const SAM_0BIT  = 0b0000_1100;

        /// Destination address is multicast address (M)
        const MCAST_COMPRESS = 0b0001_0000;

        /// Destination Address Compression (DAC) uses stateful, context-based compression.
        const DAC_STATEFULL = 0b0010_0000;

        /// if M=0 DAC=0, 128 bit destination address, carred inline
        /// if M=0 DAC=1, reserved
        /// if M=1 DAC=0, 128-bit destination address, carried inline
        /// if M=1 DAC=1, 48-bit 48 bits designed to match Unicast-Prefix-based IPv6 Multicast Addresses
        const DAM_FULL  = 0b0000_0000;
        /// if M=0 DAC=0, 64 bit destination address, first 64-bits of the address are elided.
        /// if DAC=1, 64-bit destination address, derived from context and 64 inline bits
        /// if M=1 DAC=0, 48 bit destination address in the form FFXX::00XX:XXXX:XXXX
        /// if M=1 DAC=1, reserved
        const DAM_64BIT = 0b0100_0000;
        /// if M=0 DAC=0, 16 bit destination address, first 112-bits of the address are elided.
        /// if DAC=1, 16-bit source address, derived from context and 16-bits inline
        /// if M=1 DAC=0, 32 bit destination address in the form FFXX::00XX:XXXX
        /// if M=1 DAC=1, reserved
        const DAM_16BIT = 0b1000_0000;
        /// if M=0 DAC=0, 0 bit source address, computed from encapsulating header
        /// if DAC=0, 0 bit source address, derived from context and encapsulating header
        /// if M=1 DAC=0, 8 bit destination address in the form FF02::00XX
        /// if M=1 DAC=1, reserved
        const DAM_0BIT  = 0b1100_0000;
    }
}

// TODO: complete IPHC encode/decode
impl IphcHeader {
    pub fn decode(buff: &[u8]) -> Result<(Self, usize), DecodeError> {
        unimplemented!()
    }

    pub fn encode(&self, buff: &mut[u8]) -> usize {
        unimplemented!()
    }
}

/// IPv6 HC1 Header (wireshark doesn't seem to like this?)
/// Per https://tools.ietf.org/html/rfc4944#section-10.1
#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Hc1Header {
    pub flags: Hc1Flags,
    pub hop_limit: u8,
}


bitflags::bitflags!{
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub struct Hc1Flags: u8 {
        const SRC_IF_COMPRESS  = 0b0000_0001;
        const SRC_PFX_COMPRESS = 0b0000_0010;
        const DST_IF_COMPRESS  = 0b0000_0100;
        const DST_PFX_COMPRESS = 0b0000_1000;
        const TC_COMPRESS      = 0b0001_0000;
        const NEXT_HDR_UDP     = 0b0010_0000;
        const NEXT_HDR_ICMP    = 0b0100_0000;
        const NEXT_HDR_TCP     = 0b0110_0000;
        const HC2_EN           = 0b1000_0000;

        const COMPRESS_ALL = Self::SRC_IF_COMPRESS.bits | Self::SRC_PFX_COMPRESS.bits 
            | Self::DST_IF_COMPRESS.bits | Self::DST_PFX_COMPRESS.bits | Self::TC_COMPRESS.bits;
    }
}


impl Hc1Header {
    pub fn decode(buff: &[u8]) -> Result<(Self, usize), DecodeError> {
        let mut offset = 0;

        let flags = Hc1Flags::from_bits_truncate(buff[1]);
        let hop_limit = buff[2];

        Ok((
            Self{ flags, hop_limit },
            3
        ))
    }

    pub fn encode(&self, buff: &mut[u8]) -> usize {
        // Set header and dispatch for mesh HC1
        buff[0] = HeaderType::Mesh as u8;
        buff[0] |= DispatchBits::Hc1 as u8;

        // TODO: Set HC1 flags
        buff[1] = 0;
        buff[1] |= Hc1Flags::COMPRESS_ALL.bits;

        // Hop limit always written
        buff[2] = self.hop_limit;

        // TODO: encode other header components


        return 3;
    }
}

const HEADER_MESH_SHORT_V: u8 = 0b0000_0010;
const HEADER_MESH_SHORT_F: u8 = 0b0000_0100;

/// Mesh header per [RFC4449 Section 5.2](https://tools.ietf.org/html/rfc4944#section-5.2)
#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MeshHeader {
    pub hops_left: u8,
    pub origin_addr: Address,
    pub final_addr: Address,
}

impl MeshHeader {
    pub fn decode(buff: &[u8]) -> Result<(Self, usize), DecodeError> {
        let mut offset = 0;
        let d = buff[0];

        // Check header type is correct
        if (d & HEADER_TYPE_MASK) != HeaderType::Mesh as u8 {
            // TODO: Error
        }

        // Read hops left
        let hops_left = (d >> 4) & 0x0F;

        offset += 1;

        // Read addresses
        let origin_addr = if (d & HEADER_MESH_SHORT_V) != 0 {
            let (s, n) = ShortAddress::decode(&buff[offset..])?;
            offset += n;
            Address::Short(PanId(0), s)
        } else {
            let (l, n) = ExtendedAddress::decode(&buff[offset..])?;
            offset += n;
            Address::Extended(PanId(0), l)
        };

        let final_addr = if (d & HEADER_MESH_SHORT_F) != 0 {
            let (s, n) = ShortAddress::decode(&buff[offset..])?;
            offset += n;
            Address::Short(PanId(0), s)
        } else {
            let (l, n) = ExtendedAddress::decode(&buff[offset..])?;
            offset += n;
            Address::Extended(PanId(0), l)
        };

        let h = MeshHeader{
            hops_left,
            origin_addr,
            final_addr,
        };

        Ok((h, offset))
    }

    pub fn encode(&self, buff: &mut[u8]) -> usize {
        let mut offset = 0;
        
        // Write header type
        buff[0] = HeaderType::Mesh as u8;

        // Write hops left
        buff[0] |= (self.hops_left & 0x0F) << 4;

        offset += 1;

        // Write origin address
        offset += match self.origin_addr {
            Address::Short(_p, s) => {
                buff[0] |= HEADER_MESH_SHORT_V;
                s.encode(&mut buff[offset..])
            },
            Address::Extended(_p, e) => {
                e.encode(&mut buff[offset..])
            },
            Address::None => unreachable!(),
        };

        // Write destination address
        offset += match self.origin_addr {
            Address::Short(_p, s) => {
                buff[0] |= HEADER_MESH_SHORT_V;
                s.encode(&mut buff[offset..])
            },
            Address::Extended(_p, e) => {
                e.encode(&mut buff[offset..])
            },
            Address::None => unreachable!(),
        };

        // Return new offset
        offset
    }
}

#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BroadcastHeader {
    
}

/// Fragmentation header per [rfc4944 Section 5.3](https://tools.ietf.org/html/rfc4944#section-5.3)
#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FragHeader {
    /// IP packet size prior to link-layer fragmentation
    pub datagram_size: u16,
    /// Tag to correlated datagram fragments
    pub datagram_tag: u16,
    /// Offset for fragment, only present in N>0 fragments
    pub datagram_offset: Option<u8>,
}

#[derive(Copy, Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FragHeaderKind {
    /// First fragment (no offset)
    Frag1 = 0b0000,
    /// Following fragments (including offset)
    FragN = 0b0100,
}

impl FragHeader {
    pub fn decode(buff: &[u8]) -> Result<(Self, usize), DecodeError> {
        let mut offset = 0;
        let d = buff[0];

        // Check header type is correct
        if (d & HEADER_TYPE_MASK) != HeaderType::Frag as u8 {
            // TODO: error
        }

        // Read datagram size
        let datagram_size = (buff[0] & 0b1110_0000) as u16 >> 5  | (buff[1] as u16) << 3;
        offset += 2;

        // Read datagram tag
        let datagram_tag = (buff[2] as u16) | (buff[3] as u16) >> 8;
        offset += 2;

        // For FragN, read datagram offset
        let datagram_offset = if (d & FragHeaderKind::FragN as u8) != 0 {
            offset += 1;
            Some(buff[4])
        } else {
            None
        };

        let h = FragHeader{
            datagram_size,
            datagram_tag,
            datagram_offset,
        };

        Ok((h, offset))
    }

    pub fn encode(&self, buff: &mut[u8]) -> usize {
        let mut offset = 0;
        
        // Write header type
        buff[0] = HeaderType::Frag as u8;
        // Write datagram size
        buff[0] |= ((self.datagram_size & 0b0000_0111) << 5) as u8;
        buff[1] |= (self.datagram_size >> 3) as u8;

        offset += 2;

        // Write datagram tag
        LittleEndian::write_u16(&mut buff[offset..], self.datagram_tag);
        offset += 2;

        // Write datagram offset for FragN
        if let Some(datagram_offset) = self.datagram_offset {
            buff[0] |= FragHeaderKind::FragN as u8;
            buff[offset] = datagram_offset;
            offset += 1;
        } else {
            buff[0] |= FragHeaderKind::Frag1 as u8;
        }

        // Return new offset
        offset
    }
}



#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct V6Addr(pub [u8; 16]);

impl From<Eui64> for V6Addr {
    /// Compute IPv6 Link-Local Address from EUI-64
    /// per [RFC4449 Section 7](https://tools.ietf.org/html/rfc4944#section-7)
    fn from(eui: Eui64) -> V6Addr {
        let mut buff = [0u8; 16];

        let header = 0b1111111010;
        LittleEndian::write_u64(&mut buff, header);
        LittleEndian::write_u64(&mut buff[4..], eui.0);

        V6Addr(buff)
    }
}


#[cfg(any(feature = "alloc", feature = "std"))]
impl core::fmt::Display for V6Addr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut compress = false;

        for i in 0..8 {
            let o = u16::from_be_bytes([self.0[i], self.0[i+1]]);

            match (o, compress) {
                (0, false) if i < 7 => {
                    compress = true;
                    write!(f, ":")?;
                },
                (0, true) => (),
                (_, true) => {
                    compress = false;
                    write!(f, ":{:04x}", o)?;
                },
                (_, false) if i == 0 => {
                    write!(f, "{:04x}", o)?;
                },
                (_, false) => {
                    write!(f, ":{:04x}", o)?;
                }
            }
        }
        
        Ok(())
    }
}

/// interface identifier
#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Eui64(pub u64);

impl From<(PanId, ShortAddress)> for Eui64 {
    /// Create a new EUI-64 Interface Identifier from an 802.15.4 pan_id and short address
    /// Per [RFC4449 Section 7](https://tools.ietf.org/html/rfc4944#section-6)
    fn from(a: (PanId, ShortAddress)) -> Self {
        let pan_id = a.0;
        let short_addr = a.1;

        Eui64(
            u64::from_le_bytes([
                0, 0,
                pan_id.0 as u8,
                (pan_id.0 >> 8) as u8,
                0, 0,
                short_addr.0 as u8,
                (short_addr.0 >> 8) as u8,
            ])
        )
    }
}


impl From<ExtendedAddress> for Eui64 {
    /// Create a new EUI-64 Interface Identifier from an 802.15.4 Extended address
    /// Per [RFC4449 Section 7](https://tools.ietf.org/html/rfc4944#section-6), [RFC2464 Section 4](https://tools.ietf.org/html/rfc2464)
    fn from(extended: ExtendedAddress) -> Self {
        Eui64(
            // TODO: dropping the top extended address bits, is this correct?
            u64::from_le_bytes([
                extended.0 as u8,
                (extended.0 >> 8) as u8,
                (extended.0 >> 16) as u8,
                0xFF, 0xFE,
                (extended.0 >> 24) as u8,
                (extended.0 >> 32) as u8,
                (extended.0 >> 48) as u8,
            ])
        )
    }
}


impl From<[u8; 6]> for Eui64 {
    /// Create a new EUI-64 Interface Identifier from a MAC address
    /// Per [RFC2464 Section 4](https://tools.ietf.org/html/rfc2464
    fn from(mac: [u8; 6]) -> Self {
        Eui64(
            u64::from_le_bytes([
                mac[0] ^ 0b10,  // Complement universal/local bit
                mac[1],
                mac[2],
                0xFF, 0xFE,
                mac[3],
                mac[4],
                mac[5],
            ])
        )
    }
}


// TODO: [unicast address mapping](https://tools.ietf.org/html/rfc4944#section-8)

// TODO: [multicast address mapping](https://tools.ietf.org/html/rfc4944#section-9)


// TODO: [header compression](https://tools.ietf.org/html/rfc4944#section-10)

// TODO: [IP Header Compression](https://tools.ietf.org/html/rfc6282)

#[cfg(test)]
mod test {
    use super::*;

    use std::string::ToString;

    #[test]
    fn frag_header() {
        let mut buff = [0u8; 128];

        let fh = FragHeader{
            datagram_tag: 14,
            datagram_size: 100,
            datagram_offset: Some(8),
        };

        // Encode and decode header
        let n = fh.encode(&mut buff);
        let (fh2, n2) = FragHeader::decode(&buff[..n]).unwrap();

        std::println!("Encoded: {:02x?}", &buff[..n]);

        // Check objects match
        assert_eq!(fh, fh2);
        assert_eq!(n, n2);
    }

    #[test]
    fn fmt_addr_v6() {
        let addr = V6Addr::from(Eui64::from((PanId(16), ShortAddress(24))));
        assert_eq!(addr.to_string(), "fa03:0300::0010:1000:0000");
    }
}
