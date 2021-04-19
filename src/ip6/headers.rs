
use byteorder::{ByteOrder, LittleEndian};

use ieee802154::mac::{Address, DecodeError, ExtendedAddress, PanId, ShortAddress};


// https://tools.ietf.org/html/rfc4944#page-3

#[derive(Clone, PartialEq, Debug)]
pub struct Header {
    pub mesh: Option<MeshHeader>,
    pub bcast: Option<BroadcastHeader>,
    pub frag: Option<FragHeader>,
}

impl Default for Header {
    fn default() -> Self {
        Self {
            mesh: None,
            bcast: None,
            frag: None,
        }
    }
}

impl Header {
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

        // TODO: parse out IPv6 uncompressed header
        // TODO: parse out IPv6 HC1 compressed header

        Ok(( Self{ mesh, bcast, frag }, offset ))
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

        offset
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum HeaderType {
    /// Not a LoWPAN Frame (discard packet)
    Nalp = 0b0000_0000,
    /// LoWPAN Headers
    Lowpan = 0b0000_0001,
    /// Mesh Headers
    Mesh = 0b0000_0010,
    /// Fragtmentation headers
    Frag = 0b0000_0011,
}

pub const HEADER_TYPE_MASK: u8 = 0b0000_0011;
pub const HEADER_DISPATCH_MASK: u8 = 0b1111_1100;

/// Dispatch types per [RFC4449 Section 5.1](https://tools.ietf.org/html/rfc4944#section-5.1)
// TODO: shit are these all backwards?!
#[derive(Copy, Clone, PartialEq, Debug)]
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


const HEADER_MESH_SHORT_V: u8 = 0b0000_0010;
const HEADER_MESH_SHORT_F: u8 = 0b0000_0100;

/// Mesh header per [RFC4449 Section 5.2](https://tools.ietf.org/html/rfc4944#section-5.2)
#[derive(Clone, PartialEq, Debug)]
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
pub struct BroadcastHeader {
    
}

/// Fragmentation header per [rfc4944 Section 5.3](https://tools.ietf.org/html/rfc4944#section-5.3)
#[derive(Clone, PartialEq, Debug)]
pub struct FragHeader {
    /// IP packet size prior to link-layer fragmentation
    pub datagram_size: u16,
    /// Tag to correlated datagram fragments
    pub datagram_tag: u16,
    /// Offset for fragment, only present in N>0 fragments
    pub datagram_offset: Option<u8>,
}

#[derive(Copy, Clone, PartialEq, Debug)]
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
        let datagram_size = (buff[0] as u16) << 5 & 0b111 | buff[1] as u16 >> 3;
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
        buff[0] |= ((self.datagram_size & 0b0111) >> 5) as u8;
        buff[1] |= (self.datagram_size << 5) as u8;

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
pub struct V6Addr(pub u64);

#[derive(Clone, PartialEq, Debug)]
pub struct V6LLAddr(pub [u8; 16]);


impl V6Addr {
    /// Compute IPv6 Link-Local Address per [RFC4449 Section 7](https://tools.ietf.org/html/rfc4944#section-7)
    pub fn ll_addr(&self) -> V6LLAddr {
        let mut buff = [0u8; 16];

        let header = 0b1111111010;
        LittleEndian::write_u64(&mut buff, header);
        LittleEndian::write_u64(&mut buff[4..], self.0);

        V6LLAddr(buff)
    }
}

impl From<(PanId, ShortAddress)> for V6Addr {
    /// Create a new IPv6 Link-Local Address from an 802.15.4 pan_id and short address
    /// Per [RFC4449 Section 7](https://tools.ietf.org/html/rfc4944#section-6)
    fn from(a: (PanId, ShortAddress)) -> Self {
        let pan_id = a.0;
        let short_addr = a.1;

        V6Addr(
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


impl From<ExtendedAddress> for V6Addr {
    /// Create a new IPv6 Link-Local address from an 802.15.4 Extended address
    /// Per [RFC4449 Section 7](https://tools.ietf.org/html/rfc4944#section-6), [RFC2464 Section 4](https://tools.ietf.org/html/rfc2464)
    fn from(extended: ExtendedAddress) -> Self {
        V6Addr(
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


impl From<[u8; 6]> for V6Addr {
    /// Create a new IPv6 Link-Local address from a MAC address
    /// Per [RFC2464 Section 4](https://tools.ietf.org/html/rfc2464

    fn from(mac: [u8; 6]) -> Self {
        V6Addr(
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

