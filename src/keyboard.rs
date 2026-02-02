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

pub const VOICE_GAIN: f32 = 0.4;
pub const KEY_COUNT: usize = 7;

pub struct KeyboardSynth {
    sr: u32,
    net: Box<dyn AudioUnit>,
    freqs: [Shared; KEY_COUNT],
    gates: [Shared; KEY_COUNT],
}

// net
// -------------------
// C4 - |
// D4 - |
// E4 - |
// F4 - | Join  -  chorus -
// G4 - |
// A4 - |
// B4 - |

impl KeyboardSynth {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate;
        let freqs = arr![|i| Shared::new(KEY_PITCHES[i])];
        let gates = arr![|_| Shared::new(0.0)];
        const A: f32 = 0.5;
        const D: f32 = 0.5;
        const S: f32 = 0.05;
        const R: f32 = 0.5;
        let net = Box::new(
            (var(&freqs[0])
                >> (poly_saw::<f32>() * (var(&gates[0]) >> adsr_live(A, D, S, R)) * VOICE_GAIN)
                | var(&freqs[1])
                    >> (poly_saw::<f32>()
                        * (var(&gates[1]) >> adsr_live(A, D, S, R))
                        * VOICE_GAIN)
                | var(&freqs[2])
                    >> (poly_saw::<f32>()
                        * (var(&gates[2]) >> adsr_live(A, D, S, R))
                        * VOICE_GAIN)
                | var(&freqs[3])
                    >> (poly_saw::<f32>()
                        * (var(&gates[3]) >> adsr_live(A, D, S, R))
                        * VOICE_GAIN)
                | var(&freqs[4])
                    >> (poly_saw::<f32>()
                        * (var(&gates[4]) >> adsr_live(A, D, S, R))
                        * VOICE_GAIN)
                | var(&freqs[5])
                    >> (poly_saw::<f32>()
                        * (var(&gates[5]) >> adsr_live(A, D, S, R))
                        * VOICE_GAIN)
                | var(&freqs[6])
                    >> (poly_saw::<f32>()
                        * (var(&gates[6]) >> adsr_live(A, D, S, R))
                        * VOICE_GAIN))
                >> join::<U7>()
                >> chorus(1234, 0.01, 0.05, 0.7)
                >> (pass() & feedback(delay(0.1) * 0.9)),
        );
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
