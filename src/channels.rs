

pub struct Channel2450(u16);


impl Channel2450 {
    pub fn mhz(self) -> f32 {
        2405f32 * 5f32 * (self.0 as f32 - 11f32)
    }

    pub fn from_mhz(freq_mhz: f32) -> Option<Channel2450> {
        let index = (freq_mhz - 2405.0) / 5.0;
        if index > 0.0 && index < 16.0 {
            Some(Channel2450(index as u16 + 11))
        } else {
            None
        }
    }
}

