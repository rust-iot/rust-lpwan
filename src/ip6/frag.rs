


// TODO: is it important to be able to receive more than one fragmented packet at once?
// seems... probable, in which case more buffers / a pooled approach might be better.

// Maybe useful to be able to support Minimal Fragment Forwarding / other improved approaches?
// https://tools.ietf.org/html/draft-ietf-6lo-minimal-fragment-01

use core::{char::MAX, option::Iter};

use log::debug;

use heapless::Vec;
use ieee802154::mac::{Address as MacAddress};


use super::{Header, headers::FragHeader};



pub const IPV6_MTU: usize = 1280;

pub const DEFAULT_FRAG_SIZE: usize = 64;


pub struct Frag<const MAX_FRAG_SIZE: usize> {
    buffs: [FragBuffer<MAX_FRAG_SIZE>; 4],
}


#[derive(Clone, PartialEq, Debug)]
pub enum FragState {
    None,
    Tx,
    Rx,
    Done,
}

#[derive(Clone, PartialEq, Debug)]
pub struct FragBuffer<const MAX_FRAG: usize> {
    pub state: FragState,
    pub header: Header,
    pub addr: MacAddress,
    pub tag: u16,
    pub len: usize,
    pub mask: u32,
    pub timeout: u32,
    pub offset: usize,
    pub buff: [u8; IPV6_MTU],
}

impl <const MAX_FRAG: usize> Default for FragBuffer<MAX_FRAG> {
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
            buff: [0u8; IPV6_MTU],
        }
    }
}


impl <const MAX_FRAG: usize> FragBuffer<MAX_FRAG> {

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

        std::println!("New RX fragment from: {:?} tag: {}", source, s.tag);

        s.update_rx(header, data);

        s
    }

    pub fn init_tx(dest: MacAddress, header: Header, tag: u16, data: &[u8]) -> Self {
        let mut buff = [0u8; IPV6_MTU];

        &buff[0..data.len()].copy_from_slice(data);

        std::println!("New TX fragment for: {:?} tag: {}", dest, tag);

        Self {
            state: FragState::Tx,
            header: header,
            addr: dest,
            len: data.len(),
            tag,
            buff,
            ..Default::default()
        }
    }

    pub fn num_frags(&self) -> usize {
        let mut num_frags = self.len / MAX_FRAG;
        if self.len % MAX_FRAG != 0 {
            num_frags += 1;
        }
        num_frags
    }

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
        
        // Apply fragment
        let offset = fh.datagram_offset.unwrap_or(0) as usize * 8;
        let len = data.len();
        &self.buff[offset..offset+len].copy_from_slice(data);

        // Update mask
        let index = (offset / MAX_FRAG) as u32;
        self.mask |= 1 << index;

        // Check mask for completion
        let num_frags = self.num_frags();
        let check_mask = (1 << num_frags) - 1;

        std::println!("Fragment {} rx index {} mask 0b{:08b} (check 0b{:08b})",
            self.tag, index, self.mask, check_mask);

        if self.mask == check_mask {
            std::println!("Fragment {} RX complete", self.tag);
            self.state = FragState::Done;
            true
        } else {
            false
        }
    }

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

    pub fn frag_data<'a>(&'a self, offset: usize, len: usize) -> &'a [u8] {
        &self.buff[offset..offset+len]
    }

    pub fn data<'a>(&'a self) -> &'a [u8] {
        &self.buff[..self.len]
    }
}

impl <const MAX_FRAG: usize> Iterator for FragBuffer<MAX_FRAG> {
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
            self.state = FragState::Done;
        }

        // Return fragment header / offset / data length
        Some(r)
    }
}


#[cfg(test)]
mod test {   
    use super::*;

    use std::println;
    use crate::ip6::headers::FragHeader;

    const MAX_FRAG_SIZE: usize = 64;

    #[test]
    fn fragment() {
        // Setup data to TX
        let mut tx = [0u8; 200];
        for i in 0..tx.len() {
            tx[i] = i as u8;
        }

        // Setup fragmentation buffer
        let mut frag_buff = FragBuffer::<DEFAULT_FRAG_SIZE>::init_tx(MacAddress::None, Header::default(), 0, &tx);

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
        let mut frag_buff = FragBuffer::<DEFAULT_FRAG_SIZE>::init_tx(MacAddress::None, Header::default(), 12, &tx);

        let (h1, o, l) = frag_buff.next().unwrap();

        // Setup defragmentation buffer
        let mut defrag_buff = FragBuffer::<DEFAULT_FRAG_SIZE>::init_rx(MacAddress::None, &h1, frag_buff.frag_data(o, l));

        // Transfer fragments
        while let Some((h, o, l)) = frag_buff.next() {
            defrag_buff.update_rx(&h, frag_buff.frag_data(o, l));
        }

        // Check defrag state
        assert_eq!(defrag_buff.state, FragState::Done);
        assert_eq!(frag_buff.data(), defrag_buff.data());
    }
}

