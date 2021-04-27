//! 6LoWPAN/IPv6 Fragmentation Layer
//
// https://github.com/rust-iot/rust-lpwan
// Copyright 2021 Ryan Kurte

// TODO: is it important to be able to receive more than one fragmented packet at once?
// seems... probable, in which case more buffers / a pooled approach might be better.

// Maybe useful to be able to support Minimal Fragment Forwarding / other improved approaches?
// https://tools.ietf.org/html/draft-ietf-6lo-minimal-fragment-01

use ieee802154::mac::{Address as MacAddress};

use crate::Ts;
use crate::log::{debug, warn};


use super::{Header, headers::FragHeader, SixLoError, IPV6_MTU};



/// Fragmentation buffer state
#[derive(Clone, PartialEq, Debug)]
pub enum FragState {
    None,
    Tx,
    Rx,
    Done,
}


/// Fragmentation manager, handles transmission and receipt of IPv6 datagrams
/// as fragments via 6LoWPAN.
///
/// TODO: support fragment forwarding (only runs point-to-point atm)
pub struct Frag<const MAX_FRAG_SIZE: usize> {
    config: FragConfig,
    tag: u16,
    // TODO: it would be nice to use a queue to preserve ordering...
    // unfortunately heapless::Queue doesn't have arbitrary remove
    // and heapless::Vec can only remove_swap so we can't use those anyway
    buffs: [FragBuffer<[u8; IPV6_MTU], MAX_FRAG_SIZE>; 4],
}

#[derive(Clone, PartialEq, Debug)]
pub struct FragConfig {
    pub frag_rx_timeout_ms: Ts,
    pub frag_tx_timeout_ms: Ts,
}

impl Default for FragConfig {
    fn default() -> Self {
        Self {
            frag_rx_timeout_ms: 10_000,
            frag_tx_timeout_ms: 10_000,
        }
    }
}

impl <const MAX_FRAG_SIZE: usize> Frag<MAX_FRAG_SIZE> {
    /// Create a new fragmentation manager
    pub fn new(config: FragConfig) -> Self {
        Self {
            config,
            tag: 0,
            buffs: Default::default(),
        }
    }

    /// Set-up a datagram for transmission
    pub fn transmit<E>(&mut self, now_ms: Ts, dest: MacAddress, hdr: Header, d: &[u8]) -> Result<(), SixLoError<E>> {
        // Locate a free slot in the fragment buffer
        let slot = match self.buffs.iter_mut().find(|buff| buff.state == FragState::None) {
            Some(s) => s,
            None => {
                return Err(SixLoError::NoTxFragSlots);
            }
        };

        // Initialise slot for transmission
        *slot = FragBuffer::init_tx(dest, hdr, self.tag, d);
        slot.timeout = now_ms + self.config.frag_tx_timeout_ms;

        // Increment fragment tag counter
        self.tag = self.tag.wrapping_add(1);


        Ok(())
    }

    /// Add a buffer to tracking
    fn push<E>(&mut self, fb: FragBuffer<[u8; IPV6_MTU], MAX_FRAG_SIZE>) -> Result<usize, SixLoError<E>> {
        // Find empty slot
        let slot = self.buffs.iter_mut()
            .enumerate()
            .find(|(_idx, buff)| buff.state == FragState::None);

        if let Some((idx, slot)) = slot {
            *slot = fb;
            Ok(idx)
        } else {
            Err(SixLoError::NoTxFragSlots)
        }
    }

    /// Remove a completed buffer
    pub fn pop<'a>(&'a mut self) -> Option<(&'a MacAddress, &'a Header, &'a[u8])> {
        // Find completed slot
        let slot = self.buffs.iter_mut()
            .find(|buff| buff.state == FragState::Done);
            
        let slot = match slot {
            Some(s) => s,
            None => return None,
        };

        // Clear slot state
        slot.state = FragState::None;

        // Return slot header / data
        Some((&slot.addr, &slot.header, slot.data()))
    }

    /// Handle received fragments
    pub fn receive<E>(&mut self, now_ms: Ts, src: MacAddress, hdr: &Header, d: &[u8]) -> Result<(), SixLoError<E>> {

        // Find a matching fragment buffer
        let slot_idx = hdr.frag.as_ref().map(|fh| {
            self.buffs.iter()
                .enumerate()
                .find(|(_i, buff)| {
                    buff.state == FragState::Rx && 
                    buff.addr == src &&
                    buff.tag == fh.datagram_tag
                })
                .map(|(i, _b)| i )
        }).flatten();

        match (&hdr.frag, slot_idx) {
            // Create a new buffer if no match exists
            (Some(_fh), None) => {
                // Setup new receive buffer
                let mut fb = FragBuffer::init_rx(src, hdr, d);
                fb.timeout = now_ms + self.config.frag_rx_timeout_ms;

                debug!("Fragment {} RX start", fb.tag);

                self.push(fb)?;
            },
            // Update an existing buffer if found
            (Some(_fh), Some(i)) => {
                let s = &mut self.buffs[i];
                let done = s.update_rx(hdr, d);

                if done {
                    debug!("Fragment {} RX complete", s.tag);
                    // TODO: track completed fragment stats
                    s.state = FragState::Done;
                }
            },
            // Skip fragmentation if not required
            (None, _) => {
                let fb = FragBuffer::init_done(src, hdr, d);

                self.push(fb)?;
            }
        }

        Ok(())
    }

    /// Poll for outgoing messages
    pub fn poll<'a>(&'a mut self, now_ms: Ts, opts: PollOptions) -> Option<(MacAddress, Header, &'a[u8])> {

        // Handle timeouts
        for i in 0..self.buffs.len() {
            if self.buffs[i].state == FragState::None {
                continue;
            }

            if self.buffs[i].timeout != 0 && now_ms > self.buffs[i].timeout  {
                warn!("Timeout for datagram {} via {:?}", self.buffs[i].tag, self.buffs[i].addr);

                // TODO: signal / count datagram failures

                self.buffs[i].state = FragState::None;
            }
        }

        // Update TX buffers
        for i in 0..self.buffs.len() {
            if self.buffs[i].state != FragState::Tx {
                continue;
            }

            // Check filters
            if !opts.can_tx {
                continue;
            }
            if opts.tx_addr != MacAddress::None &&
                    opts.tx_addr != self.buffs[i].addr {
                continue;
            }

            // Return fragment for TX
            if let Some((h, o, l)) = self.buffs[i].next() {
                debug!("TX fragment {} offset {}", self.buffs[i].tag, self.buffs[i].offset);

                return Some((self.buffs[i].addr, h, self.buffs[i].frag_data(o, l)))
            } else {
                debug!("TX fragment {} complete", self.buffs[i].tag);
            }
        }

        None
    }

}

/// Options for fragment polling
#[derive(Clone, PartialEq, Debug)]
pub struct PollOptions {
    /// Signals that fragments can be transmitted
    pub can_tx: bool,
    /// Filter outgoing fragments by destination address
    pub tx_addr: MacAddress,
}

impl Default for PollOptions {
    fn default() -> Self {
        Self {
            can_tx: true,
            tx_addr: MacAddress::None,
        }
    }
}

/// Fragment data storage, first step towards supporting pools / allocators
pub trait FragData: AsMut<[u8]> + AsRef<[u8]> + Clone + core::fmt::Debug {
    fn empty(size: usize) -> Self;

    fn from_bytes(data: &[u8]) -> Self;
}

// TODO: replace [u8; N] with heapless::Vec once this has const generic support
// https://github.com/japaric/heapless/issues/168
impl <const N: usize> FragData for [u8; N] {
    fn empty(_size: usize) -> Self {
        [0u8; N]
    }

    fn from_bytes(data: &[u8]) -> Self {
        let mut b = [0u8; N];

        &b[..data.len()].copy_from_slice(data);

        b
    }
}

/// Vector based fragment data where allocators are available
#[cfg(any(test, feature="alloc"))]
impl FragData for alloc::vec::Vec<u8> {
    fn empty(size: usize) -> Self {
        alloc::vec![0u8; size]
    }

    fn from_bytes(data: &[u8]) -> Self {
        data.into()
    }
}

/// Fragment buffer, contains a datagram for fragmentation and defragmentation
#[derive(Clone, PartialEq, Debug)]
pub struct FragBuffer<B: FragData, const MAX_FRAG: usize> {
    pub state: FragState,
    pub header: Header,
    pub addr: MacAddress,
    pub tag: u16,
    pub len: usize,
    pub mask: u32,
    pub timeout: Ts,
    pub offset: usize,
    pub buff: B,
}

/// Default helper for constructing new fragmentation buffer instances
impl <B: FragData, const MAX_FRAG: usize> Default for FragBuffer<B, MAX_FRAG> {
    fn default() -> Self {
        Self {
            state: FragState::None,
            addr: MacAddress::None,
            header: Header::default(),
            tag: 0,
            len: 0,
            mask: 0,
            timeout: 0,
            offset: 0,
            buff: B::empty(0),
        }
    }
}


impl <B: FragData, const MAX_FRAG: usize> FragBuffer<B, MAX_FRAG> {

    /// Initialise a fragmentation buffer in receive mode
    pub fn init_rx(source: MacAddress, header: &Header, data: &[u8]) -> Self {
        let fh = match &header.frag {
            Some(fh) => fh.clone(),
            None => unimplemented!(),
        };

        let mut s = Self {
            state: FragState::Rx,
            header: header.clone(),
            addr: source,
            tag: fh.datagram_tag,
            len: fh.datagram_size as usize,
            ..Default::default()
        };

        debug!("New RX fragment from: {:?} tag: {} ({} bytes, {} fragments)", 
                source, s.tag, s.len, s.num_frags());

        s.update_rx(header, data);

        s
    }

    /// Initialise a fragmentation buffer in transmit mode
    pub fn init_tx(dest: MacAddress, header: Header, tag: u16, data: &[u8]) -> Self {
        let buff = B::from_bytes(data);

        let mut s = Self {
            state: FragState::Tx,
            header: header,
            addr: dest,
            len: data.len(),
            tag,
            buff,
            ..Default::default()
        };

        debug!("New TX fragment for: {:?} tag: {} ({} bytes, {} fragments)", 
                dest, s.tag, s.len, s.num_frags());

        s.header.frag = None;

        s
    }

    /// Initialise fragmentation buffer with received data
    pub fn init_done(source: MacAddress, header: &Header, data: &[u8]) -> Self {
        let buff = B::from_bytes(data);

        let s = Self {
            state: FragState::Done,
            header: header.clone(),
            addr: source,
            tag: 0,
            len: data.len(),
            buff,
            ..Default::default()
        };

        debug!("New RX fragment from: {:?} tag: {} ({} bytes, complete)", 
                source, s.tag, s.len);

        s
    }

    /// Compute the number of fragments for a configured buffer
    pub fn num_frags(&self) -> usize {
        let num_frags = self.len / MAX_FRAG;
        if self.len % MAX_FRAG != 0 {
            num_frags + 1
        } else {
            num_frags
        }        
    }

    /// Handle fragment receipt
    pub fn update_rx(&mut self, header: &Header, data: &[u8]) -> bool {
        // Fetch fragment header
        let fh = match &header.frag {
            Some(fh) => fh,
            None => unimplemented!(),
        };

        // Check headers match
        // TODO: dest / src addrs as well
        if fh.datagram_tag != self.tag {
            unimplemented!()
        }

        // Merge headers (in case we receive fragments out of order)
        self.header.merge(header);
        self.header.frag = None;
        
        // Apply fragment
        let offset = fh.datagram_offset.unwrap_or(0) as usize * 8;
        let len = data.len();
        &self.buff.as_mut()[offset..offset+len].copy_from_slice(data);

        // Update mask
        self.offset = offset;
        let index = (offset / MAX_FRAG) as u32;
        self.mask |= 1 << index;

        // Check mask for completion
        let num_frags = self.num_frags();
        let check_mask = (1 << num_frags) - 1;

        #[cfg(feature = "defmt")]
        defmt::debug!("Fragment {} RX index {} mask 0b{:b} (check 0b{:b})",
            self.tag, index, self.mask, check_mask);

        #[cfg(not(feature = "defmt"))]
        log::debug!("Fragment {} RX index {} mask 0b{:08b} (check 0b{:08b})",
            self.tag, index, self.mask, check_mask);

        if self.mask == check_mask {
            debug!("Fragment {} RX complete", self.tag);
            self.state = FragState::Done;
            true
        } else {
            false
        }
    }

    /// Fetch a fragment header, offset, and data length for transmission
    pub fn frag(&self, index: usize) -> (Header, usize, usize) {

        // Setup header and offset
        let (header, offset) = match index {
            0 => {
                // First fragment contains complete header
                let h = Header {
                    frag: Some(FragHeader {
                        datagram_size: self.len as u16,
                        datagram_offset: None,
                        datagram_tag: self.tag,
                    }),
                    ..self.header.clone()
                };
                (h, 0)
            },
            _ => {
                // Later fragments only fragment header
                let o = index * MAX_FRAG;
                let h = Header{ 
                    frag: Some(FragHeader {
                        datagram_size: self.len as u16,
                        datagram_offset: Some((o / 8) as u8),
                        datagram_tag: self.tag,
                    }), 
                    ..Default::default()
                };
                (h, o)
            },
        };

        // Compute remainder and fragment length
        let remainder = self.len - offset;
        let len = MAX_FRAG.min(remainder);

        (header, offset, len)
    }

    /// Fetch fragment data given the offset and length from [`Self::frag`]
    pub fn frag_data<'a>(&'a self, offset: usize, len: usize) -> &'a [u8] {
        &self.buff.as_ref()[offset..offset+len]
    }

    /// Fetch datagram payload
    pub fn data<'a>(&'a self) -> &'a [u8] {
        &self.buff.as_ref()[..self.len]
    }
}

impl <B: FragData, const MAX_FRAG: usize> Iterator for FragBuffer<B, MAX_FRAG> {
    type Item = (Header, usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        // Check fragment is active / incomplete
        if self.state != FragState::Tx {
            return None;
        }

        // Retrieve fragment and update offset
        let r = self.frag(self.offset / MAX_FRAG);
        self.offset += MAX_FRAG;

        // Check for fragment completion
        if self.offset > self.len {
            // TODO: not sure this is the right place to set _none_
            // probably should have TxDone and RxDone options
            self.state = FragState::None;
        }

        // Return fragment header / offset / data length
        Some(r)
    }
}


#[cfg(test)]
mod test {   
    use ieee802154::mac::{PanId, ShortAddress};

    use super::*;

    use std::println;
    use crate::sixlo::{DEFAULT_FRAG_SIZE, headers::FragHeader};

    const MAX_FRAG_SIZE: usize = 64;

    #[test]
    fn fragment() {
        // Setup data to TX
        let mut tx = [0u8; 200];
        for i in 0..tx.len() {
            tx[i] = i as u8;
        }

        // Setup fragmentation buffer
        let mut frag_buff = FragBuffer::<[u8; IPV6_MTU], DEFAULT_FRAG_SIZE>::init_tx(MacAddress::None, Header::default(), 0, &tx);

        // Poll for fragments
        for j in 0..frag_buff.num_frags() {
            let (header, offset, len) = frag_buff.next().unwrap();

            println!("h: {:?} o: {} l: {}", header, offset, len);

            if j == 0 {
                // First fragment, size and no offset
                assert_eq!(header.frag, Some(FragHeader{
                    datagram_size: tx.len() as u16,
                    datagram_tag: 0,
                    datagram_offset: None,
                }));
                assert_eq!(offset, 0);
                assert_eq!(len, MAX_FRAG_SIZE);
            } else {
                // Later fragments, same size + offsets
                assert_eq!(header.frag, Some(FragHeader{
                    datagram_size: tx.len() as u16,
                    datagram_tag: 0,
                    datagram_offset: Some((j * 64 / 8) as u8),
                }));
                assert_eq!(offset, j * 64);
                assert_eq!(len, DEFAULT_FRAG_SIZE.min(tx.len() - j * DEFAULT_FRAG_SIZE));
            }
        }

        assert_eq!(frag_buff.next(), None);
    }

    #[test]
    fn defragment() {
        // Setup data to TX
        let mut tx = [0u8; 200];
        for i in 0..tx.len() {
            tx[i] = i as u8;
        }

        // Setup fragmentation buffer
        let mut frag_buff = FragBuffer::<[u8; IPV6_MTU], DEFAULT_FRAG_SIZE>::init_tx(MacAddress::None, Header::default(), 12, &tx);

        let (h1, o, l) = frag_buff.next().unwrap();

        // Setup defragmentation buffer
        let mut defrag_buff = FragBuffer::<[u8; IPV6_MTU], DEFAULT_FRAG_SIZE>::init_rx(MacAddress::None, &h1, frag_buff.frag_data(o, l));

        // Transfer fragments
        while let Some((h, o, l)) = frag_buff.next() {
            defrag_buff.update_rx(&h, frag_buff.frag_data(o, l));
        }

        // Check defrag state
        assert_eq!(defrag_buff.state, FragState::Done);
        assert_eq!(frag_buff.data(), defrag_buff.data());
    }

    #[test]
    fn frag_buffer() {
        let _ = simplelog::SimpleLogger::init(log::LevelFilter::Debug, simplelog::Config::default());

        // Setup data to TX
        let mut tx = [0u8; 200];
        for i in 0..tx.len() {
            tx[i] = i as u8;
        }

        // Setup fragment managers
        let addr_a = MacAddress::Short(PanId(1), ShortAddress(1));
        let addr_b = MacAddress::Short(PanId(1), ShortAddress(2));

        let mut frag_mgr_a = Frag::<64>::new(FragConfig::default());
        let mut frag_mgr_b = Frag::<64>::new(FragConfig::default());
        
        let mut now_ms = 0;

        // Start datagram transmission
        let h = Header{
            ..Default::default()
        };
        frag_mgr_a.transmit::<()>(now_ms, addr_b, h.clone(), &tx).unwrap();

        // Poll for fragments to TX
        let mut frag_rx = false;
        while let Some((_a, h1, d1)) = frag_mgr_a.poll(now_ms, PollOptions::default()) {
            // Receive fragments
            frag_mgr_b.receive::<()>(now_ms, addr_a, &h1, d1).unwrap();

            // Poll for complete message
            if let Some((_a, h2, d2)) = frag_mgr_b.pop() {
                // Check received data matches
                assert_eq!(&h, h2);
                assert_eq!(&tx, d2);
                frag_rx = true;
            }

            now_ms += 1;
        }

        assert!(frag_rx);
    }

    /// Test passthrough of packets that do not need to be fragmented
    #[test]
    fn frag_buffer_passthrough() {
        let _ = simplelog::SimpleLogger::init(log::LevelFilter::Debug, simplelog::Config::default());

        // Setup data to TX
        let mut tx = [0u8; 20];
        for i in 0..tx.len() {
            tx[i] = i as u8;
        }

        // Setup fragment managers
        let addr_a = MacAddress::Short(PanId(1), ShortAddress(1));
        let addr_b = MacAddress::Short(PanId(1), ShortAddress(2));

        let mut frag_mgr_a = Frag::<64>::new(FragConfig::default());
        let mut frag_mgr_b = Frag::<64>::new(FragConfig::default());
        
        let mut now_ms = 0;

        // Start datagram transmission
        let h = Header{
            ..Default::default()
        };
        frag_mgr_a.transmit::<()>(now_ms, addr_b, h.clone(), &tx).unwrap();

        // Poll for fragments to TX
        let (_a, h1, d1) = frag_mgr_a.poll(now_ms, PollOptions::default()).unwrap();

        // Receive fragments
        frag_mgr_b.receive::<()>(now_ms, addr_a, &h1, d1).unwrap();

        // Poll for complete message
        let (_a, h2, d2) = frag_mgr_b.pop().unwrap();

        // Check received data matches
        assert_eq!(&h, h2);
        assert_eq!(&tx, d2);
    }
}

