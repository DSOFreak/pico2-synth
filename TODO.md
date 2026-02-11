# TODO

## PERFORMANCE OPTIMIZATIONS

1. **Block Processing** (Biggest win - 5-10x speedup)
   - Currently calling `get_mono()` 480 times per buffer (once per sample)
   - Use `synth.net.process(BUFFER_SIZE, &input, &mut output)` for SIMD acceleration
   - Reduces function call overhead by 480x

2. **Remove Virtual Dispatch**
   - Replace `Box<dyn AudioUnit>` with generic `An<YourSynthType>`
   - Eliminates vtable lookups every sample
   - Enables full compiler inlining

3. **Control-Rate ADSR** (Major CPU savings)
   - Compute envelopes at 1kHz (keyboard scan rate) instead of 44.1kHz
   - Use linear interpolation between control points
   - 44x reduction in envelope calculations per voice

4. **Reduce Voice Count**
   - 7 voices is overkill for simple synth
   - Try 4 voices: less CPU, less memory, still plenty for most playing

5. **Simplify Audio Chain**
   - Current: `lowpole_hz >> peak` (two filters)
   - Optimized: Just `peak` or just `lowpole_hz`
   - Reduces filter overhead by 50%

## IDEAS

- Add an led for each key
  - The leds light up when the note is played -> is fun for chords and midi playback
  - Multiplexed of course
- Add some smart interface to change filter / effects variables.
  - Maybe multiple cheap oled displays
  - Maybe multiple encoders
  - The idea of encoders with surrounding leds is intriguing to me -> maybe control via i2c to reduce pin need and to keep it modular (2x 8bit variable (LED / VALUE))
- Add a pitch shift lever
- Add velocity to the keys. -> figure out how this is done in the industry and in hobby projects
  - If this is possible it would be fun to try to add it to pith shift for vibrato instead of just volume or envelope control.
