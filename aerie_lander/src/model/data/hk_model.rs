use peregrine::{Data, MaybeHash, model};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Data, MaybeHash, Serialize, Deserialize)]
pub struct InstrumentHkChannel {
    pub full_wake_rate: f64,
    pub diagnostic_wake_rate: f64,
}

impl InstrumentHkChannel {
    pub fn new(full_wake_rate: f64, diagnostic_wake_rate: f64) -> Self {
        Self {
            full_wake_rate,
            diagnostic_wake_rate,
        }
    }
}

model! {
    pub HkModel {
        // 52 bits/second; 0.1872 Mbits/hour
        pub apss: InstrumentHkChannel = InstrumentHkChannel::new(0.1872, 0.1872);
        // 27 bits/second; 0.0972 Mbits/hour
        pub idc: InstrumentHkChannel = InstrumentHkChannel::new(0.0972, 0.0972);
        // 31 bits/second; 0.1116 Mbits/hour
        pub ida: InstrumentHkChannel = InstrumentHkChannel::new(0.1116, 0.1116);
        // 16 bits/second; 0.0576 Mbits/hour
        pub heat_probe: InstrumentHkChannel = InstrumentHkChannel::new(0.0576, 0.0576);
        // 18 bits/second; 0.0648 Mbits/hour
        pub heat_probe_non_chan: InstrumentHkChannel = InstrumentHkChannel::new(0.0648, 0.0648);
        // 66 bits/second; 0.2376 Mbits/hour
        pub seis: InstrumentHkChannel = InstrumentHkChannel::new(0.2376, 0.2376);
        // 63 bits/second; 0.2268 Mbits/hour, no non-channelized data for diagnostic wakes
        pub seis_non_chan: InstrumentHkChannel = InstrumentHkChannel::new(0.2268, 0.0);
        pub dump_cmd_history: InstrumentHkChannel = InstrumentHkChannel::new(0.3123, 0.3123);
    }
}
