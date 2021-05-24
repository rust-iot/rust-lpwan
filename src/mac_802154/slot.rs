


use crate::Ts;

pub struct Slot {
    kind: Kind,
    state: State,
    config: Config,
}

pub struct Config {
    /// Total slot time
    pub slot_len: Ts,

    /// State transfer deadline
    pub deadline: Ts,
    
    /// Delay from start of slot to enabling receive mode
    pub rx_delay: Ts,

    /// Timeout for receiving in a slot
    /// 
    /// If this is defined _and_ the device can signal when _receiving_ this allows
    /// devices to sleep earlier in a possibly-active slot
    pub rx_timeout: Option<Ts>,
    
    /// Delay from RX completion to sending an ACK
    ///
    /// This provides turnaround time for the transmitter, and
    /// allows device to idle or sleep between RX and preparing for ACK TX
    pub rx_ack_delay: Ts,

    /// Delay from start of slot to enabling transmit mode
    pub tx_delay: Ts,

    /// Delay from TX complete to receiving an ACK
    pub tx_ack_delay: Ts,

    /// Timeout for receiving an ACK after transmission
    pub tx_ack_timeout: Ts,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Kind {
    Tx,
    Rx,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum State {
    Start,
    Rx,
    Tx,
    RxAck,
    TxAck,
}

pub enum Op {
    Wait(Ts),
    Cca,
    StartRx,
    StartTx,
}


impl Slot {

    pub fn update(&mut self, ts: Ts) {
        // Fetch next state and transition timeout
        let (next_state, at) = match (self.kind, self.state) {
            (Kind::Tx, State::Start) => (State::Tx, self.config.tx_delay),
            (Kind::Rx, State::Start) => (State::Rx, self.config.rx_delay),
            _ => unimplemented!()
        };

        // Handle existing state logic
        match self.state {
            State::Rx => {

            },
            State::Tx => {

            },
            _ => unimplemented!(),
        }

        // Short-circuit if we're already in the right state
        if(next_state == self.state) {
            return;
        }

        // Handle timeouts
        // Log deadline misses
        if(ts > (at + self.config.deadline)) {

        }

        // Handle state timeouts
        if(ts > at) {

        }


    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_slot_beacon() {
        
    }

    #[test]
    fn test_slot_cca() {
        // Start of slot, wait for RX delay

        // Start RX

        // Wait for TX timeout, sample RSSI

        // Sleep radio
    }

    #[test]
    fn test_slot_tsch_tx() {

        // Start of slot, wait for TX delay

        // Start TX

        // await TX completion

        // Sleep radio (if enabled)

        // wait for TX ACK delay (if enabled)

        // Start RX

        // Await ACK rx

        // Sleep radio (if enabled)
    }

    #[test]
    fn test_slot_tsch_rx() {

        // Start of slot, wait for RX delay

        // Start RX

        // await RX completion

        // wait for RX ACK delay (if required)

        // Start ACK TX

        // Await TX completion

        // Sleep radio (if enabled)
    }
}
