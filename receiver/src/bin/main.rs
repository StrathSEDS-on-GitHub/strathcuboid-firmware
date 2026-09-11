#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use core::sync::atomic::AtomicBool;

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_hal::Blocking;
use esp_hal::gpio::{Output, OutputConfig};
use esp_hal::i2c::master::I2c;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::clock::CpuClock;
use esp_radio::esp_now::{EspNowManager, EspNowReceiver, EspNowSender, PeerInfo};
use log::{error, info};
use pwm_pca9685::{Address, Pca9685};
use core::sync::atomic::Ordering;

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);
    loop {}
}

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO0
    // - GPIO2
    // - GPIO5
    // - GPIO12
    // - GPIO15
    //
    // These GPIO pins are in use by some feature of the module and should not be used.
    let _ = peripherals.GPIO6;
    let _ = peripherals.GPIO7;
    let _ = peripherals.GPIO8;
    let _ = peripherals.GPIO9;
    let _ = peripherals.GPIO10;
    let _ = peripherals.GPIO11;
    let _ = peripherals.GPIO16;
    let _ = peripherals.GPIO20;

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialized!");

    let (controller, interfaces) = esp_radio::wifi::new(peripherals.WIFI, Default::default()).unwrap();

    info!("Wifi channel: {:?}", controller.channel().unwrap());

    let esp_now = interfaces.esp_now;
    let (manager, sender, reciever) = esp_now.split();
    
    manager.set_channel(1).unwrap();

    info!("esp-now version {}", manager.version().unwrap());

    *(ESP_NOW_MANAGER.lock()).await = Some(manager);
    *(ESP_NOW_SENDER.lock()).await = Some(sender);

    let led = Output::new(peripherals.GPIO2, esp_hal::gpio::Level::High, OutputConfig::default());
    *(LED.lock()).await = Some(led);

    let i2c_bus = esp_hal::i2c::master::I2c::new(
        peripherals.I2C0, 
        esp_hal::i2c::master::Config::default().with_frequency(Rate::from_hz(1600))
    )
    .unwrap()
    .with_sda(peripherals.GPIO21)
    .with_scl(peripherals.GPIO22);

    let mut pwm = Pca9685::new(i2c_bus, Address::default()).unwrap();
    pwm.set_prescale(100).unwrap();
    pwm.enable().unwrap();

    *(PWM.lock()).await = Some(pwm);

    esp_now_command_handler(reciever).await;
}

static LED: Mutex<CriticalSectionRawMutex, Option<Output<'static>>> = Mutex::new(None);
static ESP_NOW_MANAGER: Mutex<CriticalSectionRawMutex, Option<EspNowManager<'static>>> = Mutex::new(None);
static ESP_NOW_SENDER: Mutex<CriticalSectionRawMutex, Option<EspNowSender<'static>>> = Mutex::new(None);
static CONNECTED: AtomicBool = AtomicBool::new(false);
static PWM: Mutex<CriticalSectionRawMutex, Option<Pca9685<I2c<'static, Blocking>>>> = Mutex::new(None);

async fn esp_now_send(addr: &[u8; 6], data: &[u8]) {
    let mut sender_unlocked = ESP_NOW_SENDER.lock().await;
    if let Some(s) = sender_unlocked.as_mut() {
        let _ = s.send_async(addr, data).await;
    }
}

async fn esp_now_command_handler(mut receiver: EspNowReceiver<'static>) -> ! {
    loop {
        let r = receiver.receive_async().await;
        let sender_addr = r.info.src_address; 
            
        let mut manager_unlocked = ESP_NOW_MANAGER.lock().await;
        let m = manager_unlocked.as_mut().unwrap(); 

        let connected = CONNECTED.load(Ordering::Relaxed);

        info!("Connected: {}", connected);
        info!("Recv: {:?}", r.data());

        if connected {
            let controller_addr = m.fetch_peer(true).unwrap().peer_address;
            let from_controller = controller_addr.eq(&sender_addr);

            if !from_controller {
                continue; 
            }

            if r.data().eq(b"ping") {
                if let Some(led) = LED.lock().await.as_mut() {
                    esp_now_send(&sender_addr, b"pong").await;
                    led.toggle();
                    Timer::after(Duration::from_secs(1)).await;
                    led.toggle();
                } 
            } else if r.data().eq(b"forward") {
                if let Some(pwm) = PWM.lock().await.as_mut() {
                    pwm.set_channel_on_off(pwm_pca9685::Channel::C0, 0, 4095).unwrap();
                }
            }
        } else {
            if r.data().eq(b"strathcuboid-connect") {
                info!("Trying to add peer");
                m.add_peer(PeerInfo { 
                    interface: esp_radio::esp_now::EspNowWifiInterface::Station, 
                    peer_address: sender_addr, 
                    lmk: None, 
                    channel: None, 
                    encrypt: false 
                }).unwrap();
                
                esp_now_send(&sender_addr, b"strathcuboid-connected").await; 

                CONNECTED.store(true, Ordering::Relaxed);
            }
        }
    }
}
