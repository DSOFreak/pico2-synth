extern crate alloc;
use crate::arrayinit_nostd::arr;
use alloc::boxed::Box;
use fundsp::prelude::*;

const KEY_PITCHES: [f32; 7] = [
    261.63, // C4
    293.66, // D4
    329.63, // E4
    349.23, // F4
    392.00, // G4
    440.00, // A4
    493.88, // B4
];

pub const VOICE_GAIN: f32 = 0.2;
pub const KEY_COUNT: usize = 7;

pub struct KeyboardSynth {
    sr: u32,
    net: Net,
    freqs: [Shared; KEY_COUNT],
    gates: [Shared; KEY_COUNT],
}

impl KeyboardSynth {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate;
        let mut net = Net::new(0, 1);
        let id_join = net.push(Box::new(join::<U7>()));
        let freqs = arr![|i| Shared::new(KEY_PITCHES[i])];
        let gates = arr![|_| Shared::new(0.0)];
        net.pipe_output(id_join);
        for i in 0..KEY_COUNT {
            let id = net.push(Box::new(
                var(&freqs[i]) >> (poly_saw::<f32>() * var(&gates[i]) * VOICE_GAIN),
            ));
            net.pipe_input(id);
            net.connect(id, 0, id_join, i);
        }
        Self {
            sr,
            net,
            freqs,
            gates,
        }
    }

    pub fn set_gate(&mut self, i: usize, val: f32) {
        self.gates[i].set_value(val);
    }

    #[inline(always)]
    pub fn get_sample(&mut self) -> f32 {
        self.net.get_mono()
    }
}
