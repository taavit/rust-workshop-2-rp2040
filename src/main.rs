#![no_std]
#![no_main]

use defmt::unwrap;
use embassy_executor::Spawner;
use embassy_rp::{
    adc::{Adc, Channel as AdcChannel, Config, InterruptHandler},
    gpio::Pull,
    uart::InterruptHandler as UARTInterruptHandler,
};
use embassy_rp::{bind_interrupts, peripherals::UART0, uart};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};

use {defmt_rtt as _, panic_probe as _};

static SIGNAL_CHANNEL: Channel<ThreadModeRawMutex, SignalType, 4> = Channel::new();
static PUBLISH_CHANNEL: Channel<ThreadModeRawMutex, PublishSignalType, 4> = Channel::new();

bind_interrupts!(pub struct Irqs {
    ADC_IRQ_FIFO => InterruptHandler;
    UART0_IRQ  => UARTInterruptHandler<UART0>;
});

enum SignalType {
    Sine(f32),
    Square(f32),
}

enum PublishSignalType {
    Sine(f32, f32),
    Square(f32, f32),
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    // Needed for random number generation for noise
    let mut adc = Adc::new(p.ADC, Irqs, Config::default());
    let mut p28 = AdcChannel::new_pin(p.PIN_28, Pull::None);
    let uart = uart::Uart::new(
        p.UART0,
        p.PIN_0,
        p.PIN_1,
        Irqs,
        p.DMA_CH0,
        p.DMA_CH1,
        uart::Config::default(),
    );

    unwrap!(spawner.spawn(sine_generator()));
    unwrap!(spawner.spawn(square_generator()));
    unwrap!(spawner.spawn(filter_data()));
    unwrap!(spawner.spawn(send_to_pc(uart)));
}

#[embassy_executor::task]
async fn sine_generator() {
    todo!();
}

#[embassy_executor::task]
async fn square_generator() {
    todo!();
}

#[embassy_executor::task]
async fn filter_data() {
    todo!();
}

#[embassy_executor::task]
async fn send_to_pc(mut uart: uart::Uart<'static, UART0, uart::Async>) {
    todo!();
}
