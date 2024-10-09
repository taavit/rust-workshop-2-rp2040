#![no_std]
#![no_main]

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::Either;
use embassy_rp::{
    adc::{Adc, Channel as AdcChannel, Config, InterruptHandler},
    gpio::Pull,
    uart::InterruptHandler as UARTInterruptHandler,
};
use embassy_time::Duration;

use core::{
    fmt::Write,
    num::Wrapping,
    sync::atomic::{AtomicU16, Ordering},
};
use embassy_rp::{bind_interrupts, peripherals::UART0, uart};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use heapless::String;

use {defmt_rtt as _, panic_probe as _};

static SIGNAL_CHANNEL: Channel<ThreadModeRawMutex, SignalType, 4> = Channel::new();
static PUBLISH_CHANNEL: Channel<ThreadModeRawMutex, PublishSignalType, 4> = Channel::new();
static NOISE_LEVEL: AtomicU16 = AtomicU16::new(20);

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

    let seed = adc.read(&mut p28).await.unwrap();
    unwrap!(spawner.spawn(sine_generator(seed)));
    let seed = adc.read(&mut p28).await.unwrap();
    unwrap!(spawner.spawn(square_generator(seed)));
    unwrap!(spawner.spawn(filter_data()));
    unwrap!(spawner.spawn(pc_communication(uart)));
}

#[embassy_executor::task]
async fn sine_generator(seed: u16) {
    let mut rnd = fastrand::Rng::with_seed(seed.into());
    let mut current = 0.0;
    loop {
        let normalized_level = NOISE_LEVEL.load(Ordering::Relaxed) as f32 / 100.0;
        let noise = (rnd.f32() - 0.5) * normalized_level;
        SIGNAL_CHANNEL
            .send(SignalType::Sine(libm::sinf(current) + noise))
            .await;
        embassy_time::Timer::after(Duration::from_millis(150)).await;
        current += 0.01;
    }
}

#[embassy_executor::task]
async fn square_generator(seed: u16) {
    let mut rnd = fastrand::Rng::with_seed(seed.into());
    let mut current = Wrapping(0);
    loop {
        let normalized_level = NOISE_LEVEL.load(Ordering::Relaxed) as f32 / 100.0;
        let noise = (rnd.f32() - 0.5) * normalized_level;
        if (current.0 / 50) % 2 == 0 {
            SIGNAL_CHANNEL.send(SignalType::Square(1.0 + noise)).await;
        } else {
            SIGNAL_CHANNEL.send(SignalType::Square(0.0 + noise)).await;
        }
        embassy_time::Timer::after(Duration::from_millis(50)).await;
        current += 1;
    }
}

struct Filter {
    value: f32,
}

impl Filter {
    fn new() -> Self {
        Self { value: 0.0 }
    }

    pub fn filter(&mut self, value: f32) -> f32 {
        let alpha = 0.7;
        self.value = self.value * alpha + (1.0 - alpha) * value;

        self.value
    }
}

#[embassy_executor::task]
async fn filter_data() {
    let mut sine_filter = Filter::new();
    let mut square_filter = Filter::new();
    loop {
        match SIGNAL_CHANNEL.receive().await {
            SignalType::Sine(v) => {
                PUBLISH_CHANNEL
                    .send(PublishSignalType::Sine(v, sine_filter.filter(v)))
                    .await
            }
            SignalType::Square(v) => {
                PUBLISH_CHANNEL
                    .send(PublishSignalType::Square(v, square_filter.filter(v)))
                    .await
            }
        };
    }
}

#[embassy_executor::task]
async fn pc_communication(mut uart: uart::Uart<'static, UART0, uart::Async>) {
    loop {
        let d = PUBLISH_CHANNEL.receive();
        let mut buf = String::<64>::new();
        let mut uart_buf = [0u8; 1];
        let res = embassy_futures::select::select(d, uart.read(&mut uart_buf)).await;
        match res {
            Either::First(command) => {
                match command {
                    PublishSignalType::Sine(raw, filtered) => {
                        core::write!(&mut buf, "SINE,{},{}\r\n", raw, filtered).unwrap();
                        uart.write(buf.as_bytes()).await.unwrap();
                    }
                    PublishSignalType::Square(raw, filtered) => {
                        core::write!(&mut buf, "SQUARE,{},{}\r\n", raw, filtered).unwrap();
                        uart.write(buf.as_bytes()).await.unwrap();
                    }
                };
            }
            Either::Second(_) => match uart_buf[0] {
                b'p' => {
                    let mut l = NOISE_LEVEL.load(Ordering::Relaxed);
                    l += 1;
                    let l = if l > 100 { 100 } else { l };
                    NOISE_LEVEL.store(l, Ordering::Relaxed);
                    info!("Increased noise level to {}", l);
                }
                b'l' => {
                    let mut l = NOISE_LEVEL.load(Ordering::Relaxed);
                    l -= 1;
                    let l = if l == 0 { 1 } else { l };
                    NOISE_LEVEL.store(l, Ordering::Relaxed);
                    info!("Decreased noise level to {}", l);
                }
                _ => {
                    info!("{:?}", uart_buf)
                }
            },
        }
    }
}
