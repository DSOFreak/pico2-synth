extern crate alloc;
use crate::arrayinit_nostd::arr;
use alloc::boxed::Box;
use fundsp::prelude::*;

// ============================================================================
// CONFIGURATION
// ============================================================================

pub const KEY_COUNT: usize = 9;
pub const VOICE_COUNT: usize = 7;
pub const VOICE_GAIN: f32 = 0.4;

pub const ENV_ATTACK: f32 = 0.5;
pub const ENV_DECAY: f32 = 0.5;
pub const ENV_SUSTAIN: f32 = 0.05;
pub const ENV_RELEASE: f32 = 0.5;

pub const CHORUS_SEED: u64 = 1234;
pub const CHORUS_SEPARATION: f32 = 0.01;
pub const CHORUS_VARIATION: f32 = 0.05;
pub const CHORUS_MOD_FREQ: f32 = 0.7;

pub const DELAY_TIME: f64 = 0.1;
pub const DELAY_FEEDBACK: f32 = 0.9;

const VOICE_UNASSIGNED: u8 = u8::MAX;

const KEY_PITCHES: [f32; KEY_COUNT] = [
    261.63, // C4 (button 0)
    293.66, // D4 (button 1)
    329.63, // E4 (button 2)
    349.23, // F4 (button 3)
    392.00, // G4 (button 4)
    440.00, // A4 (button 5)
    493.88, // B4 (button 6)
    523.25, // C5 (button 7)
    587.33, // D5 (button 8)
];

// ============================================================================
// SYNTHESIZER
// ============================================================================

/// Polyphonic synthesizer with 7 voices and 9 keys.
///
/// Voice allocation uses round-robin stealing: when all 7 voices are in use,
/// the next voice to be allocated cycles through 0-6 sequentially. This
/// provides fair distribution and prevents the same voice from being stolen
/// repeatedly.
///
/// When a voice is stolen, the old note is immediately cut off (gate set to 0)
/// and the new note starts with gate set to 1. The ADSR envelope will handle
/// the transition.
pub struct KeyboardSynth {
    net: Box<dyn AudioUnit>,
    freqs: [Shared; VOICE_COUNT],
    gates: [Shared; VOICE_COUNT],
    /// Maps voice index -> key index (VOICE_UNASSIGNED = free)
    voice_key: [u8; VOICE_COUNT],
    /// Next voice to steal when all are busy (round-robin counter)
    next_voice: usize,
    /// Previous key states for edge detection
    key_states: [bool; KEY_COUNT],
}

impl KeyboardSynth {
    /// Create a new synthesizer with default settings.
    pub fn new() -> Self {
        let freqs = arr![|_| Shared::new(0.0)];
        let gates = arr![|_| Shared::new(0.0)];

        // Audio network: 7 voices in parallel -> join -> chorus -> delay
        // Each voice: frequency input -> poly_saw oscillator -> ADSR envelope -> gain
        let net = Box::new(
            (var(&freqs[0])
                >> (poly_saw::<f32>()
                    * (var(&gates[0])
                        >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                    * VOICE_GAIN)
                | var(&freqs[1])
                    >> (poly_saw::<f32>()
                        * (var(&gates[1])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN)
                | var(&freqs[2])
                    >> (poly_saw::<f32>()
                        * (var(&gates[2])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN)
                | var(&freqs[3])
                    >> (poly_saw::<f32>()
                        * (var(&gates[3])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN)
                | var(&freqs[4])
                    >> (poly_saw::<f32>()
                        * (var(&gates[4])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN)
                | var(&freqs[5])
                    >> (poly_saw::<f32>()
                        * (var(&gates[5])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN)
                | var(&freqs[6])
                    >> (poly_saw::<f32>()
                        * (var(&gates[6])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN))
                >> join::<U7>()
                >> chorus(
                    CHORUS_SEED,
                    CHORUS_SEPARATION,
                    CHORUS_VARIATION,
                    CHORUS_MOD_FREQ,
                )
                >> (pass() & feedback(delay(DELAY_TIME) * DELAY_FEEDBACK)),
        );

        Self {
            net,
            freqs,
            gates,
            voice_key: [VOICE_UNASSIGNED; VOICE_COUNT],
            next_voice: 0,
            key_states: [false; KEY_COUNT],
        }
    }

    /// Poll all keys and handle voice allocation.
    ///
    /// Call this at ~50Hz (20ms interval) from your main loop.
    /// Pass a closure that returns true if the key at given index is pressed.
    ///
    /// # Performance
    /// - Worst case: 9 keys × (1 closure call + 1 comparison + up to 7 voice checks)
    /// - ~72 operations per poll at 50Hz = 3,600 ops/second (negligible)
    /// - Sample generation is the hot path at 44.1kHz
    #[inline]
    pub fn poll<F: Fn(usize) -> bool>(&mut self, is_pressed: F) {
        for key in 0..KEY_COUNT {
            let pressed = is_pressed(key);
            if pressed != self.key_states[key] {
                self.key_states[key] = pressed;
                self.handle_key_change(key, pressed);
            }
        }
    }

    /// Handle a key press or release event.
    #[inline]
    fn handle_key_change(&mut self, key: usize, pressed: bool) {
        let key_u8 = key as u8;

        if pressed {
            // Check if this key already has a voice (retrigger same note)
            for voice in 0..VOICE_COUNT {
                if self.voice_key[voice] == key_u8 {
                    // Retrigger: reset envelope to attack phase
                    self.gates[voice].set_value(1.0);
                    return;
                }
            }

            // Find first free voice
            for voice in 0..VOICE_COUNT {
                if self.voice_key[voice] == VOICE_UNASSIGNED {
                    self.allocate_voice(voice, key, key_u8);
                    return;
                }
            }

            // All voices busy - steal using round-robin
            let voice = self.next_voice;
            self.next_voice = (self.next_voice + 1) % VOICE_COUNT;
            // Note: Stealing immediately cuts off the old note
            // The ADSR will handle the transition
            self.allocate_voice(voice, key, key_u8);
        } else {
            // Key released - find its voice and enter release phase
            for voice in 0..VOICE_COUNT {
                if self.voice_key[voice] == key_u8 {
                    self.gates[voice].set_value(0.0);
                    // Voice stays assigned until reallocated
                    // This allows envelope release to complete
                    break;
                }
            }
        }
    }

    /// Allocate a voice to a key and trigger the envelope.
    #[inline(always)]
    fn allocate_voice(&mut self, voice: usize, key: usize, key_u8: u8) {
        self.voice_key[voice] = key_u8;
        self.freqs[voice].set_value(KEY_PITCHES[key]);
        self.gates[voice].set_value(1.0);
    }

    /// Generate next audio sample.
    ///
    /// # Performance
    /// Hot path - called at sample rate (44.1kHz).
    /// Marked #[inline(always)] to ensure monomorphization.
    #[inline(always)]
    pub fn get_sample(&mut self) -> f32 {
        self.net.get_mono()
    }
}
