


// TODO: is it important to be able to receive more than one fragmented packet at once?
// seems... probable, in which case more buffers / a pooled approach might be better.

// Maybe useful to be able to support Minimal Fragment Forwarding / other improved approaches?
// https://tools.ietf.org/html/draft-ietf-6lo-minimal-fragment-01

use core::option::Iter;

use log::debug;

use heapless::Vec;
use ieee802154::mac::{Address as MacAddress};


use super::{Header, headers::FragHeader};



const IPV6_MTU: usize = 1280;

const MAX_FRAG_SIZE: usize = 64;



#[derive(Clone, PartialEq, Debug)]
pub enum FragRxState {
    None,
    Pending{
        // Headers included in the first fragment so must be stored
        header: Header,

        tag: u16,
        size: u16,
        mask: u32,

        timeout: u32,
    }
}

pub struct FragRxBuffer {
    pub state: FragRxState,
    pub buff: [u8; IPV6_MTU],
}

impl Default for FragRxBuffer {
    fn default() -> Self {
        Self {
            state: FragRxState::None,
            buff: [0u8; IPV6_MTU],
        }
    }
}


impl FragRxBuffer {

    fn split(&mut self, header: Header, data: &[u8]) -> () {
        
    }

}


#[derive(Clone, PartialEq, Debug)]
pub enum FragTxState {
    None,
    Sending,
    Done,
}

pub struct FragTxBuffer<const MAX_FRAG: usize> {
    pub state: FragTxState,
    pub dest: MacAddress,
    pub header: Header,
    pub tag: u16,
    pub len: usize,
    pub offset: usize,
    pub buff: [u8; IPV6_MTU],
}

impl <const MAX_FRAG: usize> Default for FragTxBuffer<MAX_FRAG> {
    fn default() -> Self {
        Self {
            dest: MacAddress::None,
            header: Header::default(),
            state: FragTxState::None,
            len: 0,
            tag: 0,
            offset: 0,
            buff: [0u8; IPV6_MTU],
        }
    }
}

impl <const MAX_FRAG: usize> FragTxBuffer<MAX_FRAG> {
    pub fn init(dest: MacAddress, header: Header, tag: u16, data: &[u8]) -> Self {
        let mut buff = [0u8; IPV6_MTU];

        &buff[0..data.len()].copy_from_slice(data);

        Self {
            state: FragTxState::Sending,
            header: header,
            dest: dest,
            len: data.len(),
            tag,
            offset: 0,
            buff,
        }
    }

    pub fn num_frags(&self) -> usize {
        let mut num_frags = self.len / MAX_FRAG;
        if self.len % MAX_FRAG != 0 {
            num_frags += 1;
        }
        num_frags
    }
}

impl <const MAX_FRAG: usize> Iterator for FragTxBuffer<MAX_FRAG> {
    type Item = (Header, usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if self.state != FragTxState::Sending {
            return None;
        }

        // Setup fragment header
        let mut fh = FragHeader {
            datagram_size: self.len as u16,
            datagram_offset: None,
            datagram_tag: 0,
        };

        if self.offset == 0 {

            // First packet, fragment header with no tag
            let h = Header {
                frag: Some(FragHeader {
                    datagram_size: self.len as u16,
                    datagram_offset: None,
                    datagram_tag: self.tag,
                }),
                ..self.header.clone()
            };

            // Compute fragment data size
            let frag_size = MAX_FRAG.min(self.len);

            // Update fragment state
            self.offset += MAX_FRAG;

            // Return header / offset / length
            Some((h, 0, frag_size))

        } else {
            // Further packets, only fragment header
            let h = Header{ 
                frag: Some(FragHeader {
                    datagram_size: self.len as u16,
                    datagram_offset: Some((self.offset / 8) as u8),
                    datagram_tag: self.tag,
                }), 
                ..Default::default()
            };

            let offset = self.offset;
            let remainder = self.len - offset;
            let frag_size = MAX_FRAG.min(remainder);

            // Update fragment state
            self.offset += MAX_FRAG;

            if self.offset >= self.len {
                self.state = FragTxState::Done;
            }            

            // Return header / offset / length
            Some((h, offset, frag_size))
        }
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
        let mut frag_buff = FragTxBuffer::<MAX_FRAG_SIZE>::init(MacAddress::None, Header::default(), 0, &tx);

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
                assert_eq!(len, MAX_FRAG_SIZE.min(tx.len() - j * MAX_FRAG_SIZE));
            }
        }

        assert_eq!(frag_buff.next(), None);
    }

    #[test]
    fn defragment() {

    }

}

