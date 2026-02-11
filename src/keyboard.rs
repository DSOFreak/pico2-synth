extern crate alloc;
use crate::arrayinit_nostd::arr;
use alloc::boxed::Box;
use fundsp::prelude::*;

// ============================================================================
// CONFIGURATION
// ============================================================================

/// 12 keys = full chromatic octave (C, C#, D, D#, E, F, F#, G, G#, A, A#, B)
pub const KEY_COUNT: usize = 12;
pub const VOICE_COUNT: usize = 7;
pub const VOICE_GAIN: f32 = 0.4;

/// 4 octaves multiplexed via output scanning
/// Octave 0 = C3-B3, Octave 1 = C4-B4 (middle), Octave 2 = C5-B5, Octave 3 = C6-B6
pub const OCTAVE_COUNT: usize = 4;

pub const ENV_ATTACK: f32 = 0.5;
pub const ENV_DECAY: f32 = 0.5;
pub const ENV_SUSTAIN: f32 = 0.5;
pub const ENV_RELEASE: f32 = 0.5;

pub const CHORUS_SEED: u64 = 1234;
pub const CHORUS_SEPARATION: f32 = 0.01;
pub const CHORUS_VARIATION: f32 = 0.05;
pub const CHORUS_MOD_FREQ: f32 = 0.7;

pub const DELAY_TIME: f64 = 0.1;
pub const DELAY_FEEDBACK: f32 = 0.9;
pub const LP_CUTOFF: f32 = 1500.0;

/// Voice unassigned marker
const VOICE_UNASSIGNED: u8 = u8::MAX;

/// Precomputed frequencies for all 12 semitones in octave 0 (C3-B3)
const SEMITONE_FREQS: [f32; KEY_COUNT] = [
    130.81, // C  (C3)
    138.59, // C# (C#3)
    146.83, // D  (D3)
    155.56, // D# (D#3)
    164.81, // E  (E3)
    174.61, // F  (F3)
    185.00, // F# (F#3)
    196.00, // G  (G3)
    207.65, // G# (G#3)
    220.00, // A  (A3)
    233.08, // A# (A#3)
    246.94, // B  (B3)
];

// ============================================================================
// VOICE NOTE ENCODING
// ============================================================================

/// Encode key and octave into a single u8 for voice tracking.
/// Encoding: (octave << 4) | key
#[inline(always)]
const fn encode_note(key: u8, octave: u8) -> u8 {
    (octave << 4) | (key & 0x0F)
}

// ============================================================================
// SYNTHESIZER
// ============================================================================

/// Polyphonic synthesizer with multiplexed 48-key matrix (12 keys × 4 octaves).
///
/// Hardware setup:
/// - 12 physical key inputs (buttons)
/// - 4 octave select outputs (only one LOW at a time to enable that octave)
/// - Scanning through octaves rapidly gives us 48 virtual keys
///
/// Features:
/// - Full 4-octave range (C3-B3 up to C6-B6)
/// - Octave multiplexing: same physical key can trigger different octaves
/// - Round-robin voice stealing when all 7 voices are busy
/// - Rapid octave scanning to catch all key presses
///
/// The synth scans through all 4 octaves on each poll, setting one output
/// LOW at a time and reading the 12 keys for that octave.
pub struct KeyboardSynth {
    net: Box<dyn AudioUnit>,
    freqs: [Shared; VOICE_COUNT],
    gates: [Shared; VOICE_COUNT],
    /// Maps voice index -> encoded note (key + octave), or VOICE_UNASSIGNED
    voice_note: [u8; VOICE_COUNT],
    /// Next voice to steal when all are busy (round-robin counter)
    next_voice: usize,
    /// Previous key states for edge detection (per octave)
    key_states: [[bool; KEY_COUNT]; OCTAVE_COUNT],
    pitch_bend: Shared,
    resonator_freq: Shared,
}

impl KeyboardSynth {
    /// Create a new synthesizer with default settings.
    pub fn new() -> Self {
        let freqs = arr![|_| Shared::new(0.0)];
        let gates = arr![|_| Shared::new(0.0)];
        let pitch_bend = Shared::new(1.0);
        let resonator_freq = Shared::new(880.0);
        let net = Box::new(
            (var(&freqs[0]) * var(&pitch_bend)
                >> (poly_saw::<f32>()
                    * (var(&gates[0])
                        >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                    * VOICE_GAIN)
                | var(&freqs[1]) * var(&pitch_bend)
                    >> (poly_saw::<f32>()
                        * (var(&gates[1])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN)
                | var(&freqs[2]) * var(&pitch_bend)
                    >> (poly_saw::<f32>()
                        * (var(&gates[2])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN)
                | var(&freqs[3]) * var(&pitch_bend)
                    >> (poly_saw::<f32>()
                        * (var(&gates[3])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN)
                | var(&freqs[4]) * var(&pitch_bend)
                    >> (poly_saw::<f32>()
                        * (var(&gates[4])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN)
                | var(&freqs[5]) * var(&pitch_bend)
                    >> (poly_saw::<f32>()
                        * (var(&gates[5])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN)
                | var(&freqs[6]) * var(&pitch_bend)
                    >> (poly_saw::<f32>()
                        * (var(&gates[6])
                            >> adsr_live(ENV_ATTACK, ENV_DECAY, ENV_SUSTAIN, ENV_RELEASE))
                        * VOICE_GAIN))
                >> join::<U7>()
                >> lowpole_hz(LP_CUTOFF)
                >> (pass() | var(&resonator_freq) | dc(0.8))
                >> resonator::<f32>(), //   >> chorus(
                                       //       CHORUS_SEED,
                                       //       CHORUS_SEPARATION,
                                       //       CHORUS_VARIATION,
                                       //       CHORUS_MOD_FREQ,
                                       //   ),
                                       //>> (pass() & feedback(delay(DELAY_TIME) * DELAY_FEEDBACK)),
        );

        Self {
            net,
            freqs,
            gates,
            voice_note: [VOICE_UNASSIGNED; VOICE_COUNT],
            next_voice: 0,
            key_states: [[false; KEY_COUNT]; OCTAVE_COUNT],
            pitch_bend,
            resonator_freq,
        }
    }

    /// Scan all octaves and handle key detection.
    /// Update key state and handle press/release events.
    /// This should be called on every scan with the current key state.
    /// It will detect edge changes and trigger note on/off accordingly.
    #[inline]
    pub fn update_key(&mut self, key: usize, octave: u8, pressed: bool) {
        let octave_idx = octave as usize;
        if pressed != self.key_states[octave_idx][key] {
            self.key_states[octave_idx][key] = pressed;
            self.handle_key_change(key, octave, pressed);
        }
    }

    /// Handle a key press or release event.
    /// This is called internally when a state change is detected.
    #[inline]
    fn handle_key_change(&mut self, key: usize, octave: u8, pressed: bool) {
        let note = encode_note(key as u8, octave);
        let octave_mult = 1 << octave; // 2^octave

        if pressed {
            // Check if this exact note (key + octave) already has a voice
            for voice in 0..VOICE_COUNT {
                if self.voice_note[voice] == note {
                    self.gates[voice].set_value(1.0);
                    return;
                }
            }

            // Find first free voice
            for voice in 0..VOICE_COUNT {
                if self.voice_note[voice] == VOICE_UNASSIGNED {
                    self.allocate_voice(voice, note, key, octave_mult);
                    return;
                }
            }

            // All voices busy - steal using round-robin
            let voice = self.next_voice;
            self.next_voice = (self.next_voice + 1) % VOICE_COUNT;
            self.allocate_voice(voice, note, key, octave_mult);
        } else {
            // Key released - find the voice with this exact note
            for voice in 0..VOICE_COUNT {
                if self.voice_note[voice] == note {
                    self.gates[voice].set_value(0.0);
                    break;
                }
            }
        }
    }
    /// Allocate a voice to a note and trigger the envelope.
    #[inline(always)]
    fn allocate_voice(&mut self, voice: usize, note: u8, key: usize, octave_mult: u8) {
        self.voice_note[voice] = note;
        let freq = SEMITONE_FREQS[key] * octave_mult as f32;
        self.freqs[voice].set_value(freq);
        self.gates[voice].set_value(1.0);
    }

    /// Generate next audio sample.
    #[inline(always)]
    pub fn get_sample(&mut self) -> f32 {
        self.net.get_mono()
    }

    /// Set pitch bend. Input range: -1.0 (1 semitone down) to 1.0 (1 semitone up).
    /// Uses cheap linear approximation: ratio ≈ 1 + bend * ln(2)/12
    #[inline]
    pub fn set_pitch_bend(&self, bend: f32) {
        assert!(bend >= -12.0 && bend <= 12.0, "Pitch bend out of range");
        // ln(2)/12 ≈ 0.05776, gives ~0.16% max error for ±1 semitone
        const BEND_FACTOR: f32 = 0.057762265;
        let ratio = 1.0 + bend * BEND_FACTOR;
        self.pitch_bend.set_value(ratio);
    }

    /// Get a clone of the pitch bend Shared for external control
    #[inline]
    pub fn pitch_bend_control(&self) -> Shared {
        self.pitch_bend.clone()
    }
    #[inline]
    pub fn resonator_freq_control(&self) -> Shared {
        self.resonator_freq.clone()
    }
}
