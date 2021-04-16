

/// 2.4GHz channel
pub struct Ch2450(u16);


impl Ch2450 {
    /// Fetch the channel frequency in MHz
    pub fn mhz(self) -> f32 {
        2405f32 * 5f32 * (self.0 as f32 - 11f32)
    }

    /// Attempt to convert a channel frequency into a channel index
    pub fn from_mhz(freq_mhz: f32) -> Option<Ch2450> {
        let index = (freq_mhz - 2405.0) / 5.0;
        if index > 0.0 && index < 16.0 {
            Some(Ch2450(index as u16 + 11))
        } else {
            None
        }
    }
}

/// 2.45 GHz Channel Pages
pub const CHANNEL_PAGES_2450: &'static [&'static [u16]] = &[
    // Page 0
    &[11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26],
];

