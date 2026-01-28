extern crate alloc;
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

pub struct Voice {
    pub gate: bool,
    unit: Box<dyn AudioUnit>,
}

pub struct KeyboardSynth {
    pub voices: [Voice; KEY_COUNT],
}

impl Voice {
    #[allow(clippy::precedence)]
    pub fn new(frequency: f32, sample_rate: f64) -> Self {
        // Following keys.rs pattern: pitch >> poly_saw() * gain
        let mut unit: Box<dyn AudioUnit> =
            Box::new(dc(frequency) >> poly_saw::<f32>() * VOICE_GAIN);
        unit.set_sample_rate(sample_rate);
        unit.allocate();
        Self { gate: false, unit }
    }

    #[inline(always)]
    pub fn get_sample(&mut self) -> f32 {
        if self.gate { self.unit.get_mono() } else { 0.0 }
    }
}

impl KeyboardSynth {
    #[inline(always)]
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate as f64;
        Self {
            voices: KEY_PITCHES.map(|freq| Voice::new(freq, sr)),
        }
    }

    #[inline(always)]
    pub fn get_sample(&mut self) -> f32 {
        let mut output = 0.0;
        for voice in self.voices.iter_mut() {
            output += voice.get_sample();
        }
        output
    }
}
