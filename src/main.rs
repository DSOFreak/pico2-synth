//! This example shows generating audio and sending it to a connected i2s DAC using the PIO
//! module of the RP2040.
//!
//! Connect the i2s DAC as follows:
//!   bclk : GPIO 18
//!   lrc  : GPIO 19
//!   din  : GPIO 20
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
use embassy_rp::gpio::Input;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::pio_programs::i2s::{PioI2sOut, PioI2sOutProgram};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

mod arrayinit_nostd;
mod keyboard;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

const SAMPLE_RATE: u32 = 44_100;
const BIT_DEPTH: u32 = 16;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    unsafe {
        ALLOCATOR.lock().init_from_slice(&mut HEAP);
    }

    // Setup pio state machine for i2s output
    let Pio {
        mut common, sm0, ..
    } = Pio::new(p.PIO0, Irqs);

    let bit_clock_pin = p.PIN_18;
    let left_right_clock_pin = p.PIN_19;
    let data_pin = p.PIN_20;

    let mut busy_pin = embassy_rp::gpio::Output::new(p.PIN_16, embassy_rp::gpio::Level::Low);

    let input0 = Input::new(p.PIN_0, embassy_rp::gpio::Pull::Up);
    let input1 = Input::new(p.PIN_1, embassy_rp::gpio::Pull::Up);
    let input2 = Input::new(p.PIN_2, embassy_rp::gpio::Pull::Up);
    let input3 = Input::new(p.PIN_3, embassy_rp::gpio::Pull::Up);
    let input4 = Input::new(p.PIN_4, embassy_rp::gpio::Pull::Up);
    let input5 = Input::new(p.PIN_5, embassy_rp::gpio::Pull::Up);
    let input6 = Input::new(p.PIN_6, embassy_rp::gpio::Pull::Up);

    let inputs = [
        &input0, &input1, &input2, &input3, &input4, &input5, &input6,
    ];

    let mut synth = keyboard::KeyboardSynth::new(SAMPLE_RATE);

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
    let mut last_poll = Instant::now();
    const POLL_INTERVAL: embassy_time::Duration = embassy_time::Duration::from_millis(20);

    loop {
        // trigger transfer of front buffer data to the pio fifo
        // but don't await the returned future, yet
        let dma_future = i2s.write(front_buffer);

        busy_pin.set_high();

        // Poll the keyboard inputs at 50Hz (every 20ms)
        if last_poll.elapsed() >= POLL_INTERVAL {
            last_poll = Instant::now();
            let mut i = 0;
            while i < keyboard::KEY_COUNT {
                synth.set_gate(i, if inputs[i].is_low() { 1.0 } else { 0.0 });
                i += 1;
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
