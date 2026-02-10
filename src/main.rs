//! This example shows generating audio and sending it to a connected i2s DAC using the PIO
//! module of the RP2040.
//!
//! Connect the i2s DAC as follows:
//!   bclk : GPIO 18
//!   lrc  : GPIO 19
//!   din  : GPIO 20
//!
//! I2C device (vl53l0x) connected to:
//!   sda  : GPIO 26
//!   scl  : GPIO 27
//!
//! Then hold down the boot select button to trigger a rising triangle waveform.

#![no_std]
#![no_main]
#![allow(static_mut_refs)]

extern crate alloc;
use core::mem;
use embassy_executor::Spawner;

use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

const HEAP_SIZE: usize = 384 * 1024;
static mut HEAP: [mem::MaybeUninit<u8>; HEAP_SIZE] = [mem::MaybeUninit::uninit(); HEAP_SIZE];

use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::i2c::{Async, I2c, InterruptHandler as I2cInterruptHandler};
use embassy_rp::peripherals::{I2C1, PIO0};
use embassy_rp::pio::{InterruptHandler as PioInterruptHandler, Pio};
use embassy_rp::pio_programs::i2s::{PioI2sOut, PioI2sOutProgram};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

use vl53l0x::VL53L0x;

mod arrayinit_nostd;
mod keyboard;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => PioInterruptHandler<PIO0>;
    I2C1_IRQ => I2cInterruptHandler<I2C1>;
});

const SAMPLE_RATE: u32 = 44_100;
const BIT_DEPTH: u32 = 16;

// Task to handle VL53L0X interrupts via async GPIO
#[embassy_executor::task]
async fn sensor_task(mut tof: VL53L0x<I2c<'static, I2C1, Async>>, mut int_pin: Input<'static>) {
    loop {
        // Wait for falling edge on GPIO1 (measurement ready)
        int_pin.wait_for_falling_edge().await;

        // Read and print distance
        match tof.read_range_continuous_millimeters() {
            Ok(distance) => defmt::info!("VL53L0X: {} mm", distance),
            Err(_) => defmt::warn!("VL53L0X read failed"),
        }
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    unsafe {
        ALLOCATOR.lock().init_from_slice(&mut HEAP);
    }

    // Setup I2C1 for vl53l0x on GPIO 26 (SDA) and GPIO 27 (SCL)
    let i2c = I2c::new_async(
        p.I2C1,
        p.PIN_27,
        p.PIN_26,
        Irqs,
        embassy_rp::i2c::Config::default(),
    );

    // Initialize vl53l0x time-of-flight sensor
    let mut tof = VL53L0x::new(i2c).expect("VL53L0X initialization failed");
    defmt::info!("VL53L0X sensor initialized successfully");

    // Configure sensor timing (200ms budget for better accuracy)
    tof.set_measurement_timing_budget(200000)
        .expect("Failed to set timing budget");

    // Start continuous mode
    tof.start_continuous(0)
        .expect("Failed to start continuous mode");

    // Configure GP22 as input for VL53L0X GPIO1 (async interrupt)
    let tof_int_pin = Input::new(p.PIN_22, Pull::Up);
    defmt::info!("VL53L0X interrupt on GP22");

    // Spawn sensor interrupt handler task
    _spawner.spawn(sensor_task(tof, tof_int_pin)).unwrap();

    // Setup pio state machine for i2s output
    let Pio {
        mut common, sm0, ..
    } = Pio::new(p.PIO0, Irqs);

    let bit_clock_pin = p.PIN_18;
    let left_right_clock_pin = p.PIN_19;
    let data_pin = p.PIN_20;

    let mut busy_pin = embassy_rp::gpio::Output::new(p.PIN_16, embassy_rp::gpio::Level::Low);

    // 12 keys for full chromatic octave (C, C#, D, D#, E, F, F#, G, G#, A, A#, B)
    let input0 = Input::new(p.PIN_0, embassy_rp::gpio::Pull::Up);
    let input1 = Input::new(p.PIN_1, embassy_rp::gpio::Pull::Up);
    let input2 = Input::new(p.PIN_2, embassy_rp::gpio::Pull::Up);
    let input3 = Input::new(p.PIN_3, embassy_rp::gpio::Pull::Up);
    let input4 = Input::new(p.PIN_4, embassy_rp::gpio::Pull::Up);
    let input5 = Input::new(p.PIN_5, embassy_rp::gpio::Pull::Up);
    let input6 = Input::new(p.PIN_6, embassy_rp::gpio::Pull::Up);
    let input7 = Input::new(p.PIN_7, embassy_rp::gpio::Pull::Up);
    let input8 = Input::new(p.PIN_8, embassy_rp::gpio::Pull::Up);
    let input9 = Input::new(p.PIN_9, embassy_rp::gpio::Pull::Up);
    let input10 = Input::new(p.PIN_10, embassy_rp::gpio::Pull::Up);
    let input11 = Input::new(p.PIN_11, embassy_rp::gpio::Pull::Up);

    let inputs: [&Input<'_>; keyboard::KEY_COUNT] = [
        &input0, &input1, &input2, &input3, &input4, &input5, &input6, &input7, &input8, &input9,
        &input10, &input11,
    ];

    // 4 octave select outputs (only one LOW at a time to enable that octave)
    let mut octave0_en = embassy_rp::gpio::Output::new(p.PIN_12, embassy_rp::gpio::Level::High);
    let mut octave1_en = embassy_rp::gpio::Output::new(p.PIN_13, embassy_rp::gpio::Level::High);
    let mut octave2_en = embassy_rp::gpio::Output::new(p.PIN_14, embassy_rp::gpio::Level::High);
    let mut octave3_en = embassy_rp::gpio::Output::new(p.PIN_15, embassy_rp::gpio::Level::High);

    let mut synth = keyboard::KeyboardSynth::new();

    let program = PioI2sOutProgram::new(&mut common);
    let mut i2s = PioI2sOut::new(
        &mut common,
        sm0,
        p.DMA_CH0,
        data_pin,
        bit_clock_pin,
        left_right_clock_pin,
        SAMPLE_RATE,
        BIT_DEPTH,
        &program,
    );

    // create two audio buffers (back and front) which will take turns being
    // filled with new audio data and being sent to the pio fifo using dma
    const BUFFER_SIZE: usize = 480;
    static DMA_BUFFER: StaticCell<[u32; BUFFER_SIZE * 2]> = StaticCell::new();
    let dma_buffer = DMA_BUFFER.init_with(|| [0u32; BUFFER_SIZE * 2]);
    let (mut back_buffer, mut front_buffer) = dma_buffer.split_at_mut(BUFFER_SIZE);

    // start pio state machine
    use embassy_time::Instant;
    let mut last_scan = Instant::now();
    // Scan at ~1kHz to properly read all 48 keys (12 keys × 4 octaves)
    const SCAN_INTERVAL: embassy_time::Duration = embassy_time::Duration::from_micros(250);

    loop {
        // trigger transfer of front buffer data to the pio fifo
        // but don't await the returned future, yet
        let dma_future = i2s.write(front_buffer);

        busy_pin.set_high();

        // Scan the keyboard matrix at ~1kHz
        // Each scan cycles through all 4 octaves
        if last_scan.elapsed() >= SCAN_INTERVAL {
            last_scan = Instant::now();

            // Scan all 4 octaves
            // For each octave: enable it (set output LOW), read 12 keys, disable it (set HIGH)
            for octave in 0..keyboard::OCTAVE_COUNT as u8 {
                // Enable this octave
                match octave {
                    0 => octave0_en.set_low(),
                    1 => octave1_en.set_low(),
                    2 => octave2_en.set_low(),
                    3 => octave3_en.set_low(),
                    _ => {}
                }

                // Read all 12 keys for this octave
                for key in 0..keyboard::KEY_COUNT {
                    let pressed = inputs[key].is_low();
                    synth.update_key(key, octave, pressed);
                }

                // Disable this octave
                match octave {
                    0 => octave0_en.set_high(),
                    1 => octave1_en.set_high(),
                    2 => octave2_en.set_high(),
                    3 => octave3_en.set_high(),
                    _ => {}
                }
            }
        }

        // fill back buffer with fresh audio samples before awaiting the dma future
        for s in back_buffer.iter_mut() {
            let sample = (synth.get_sample() * 32767.0) as i16;
            // duplicate mono sample into lower and upper half of dma word
            *s = (sample as u16 as u32) * 0x10001;
        }

        busy_pin.set_low();

        // now await the dma future. once the dma finishes, the next buffer needs to be queued
        // within DMA_DEPTH / SAMPLE_RATE - seconds
        dma_future.await;
        mem::swap(&mut back_buffer, &mut front_buffer);
    }
}
