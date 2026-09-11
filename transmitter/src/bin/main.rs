#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering;

use embassy_sync::mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;

use embassy_executor::Spawner;

use esp_radio::esp_now::BROADCAST_ADDRESS;
use esp_radio::esp_now::EspNowError;
use esp_radio::esp_now::EspNowManager;
use esp_radio::esp_now::EspNowReceiver;
use esp_radio::esp_now::EspNowSender;
use esp_radio::esp_now::PeerInfo;
use log::info;
use log::error;

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

    futures_util::join!(
        esp_now_command_handler(reciever),
        connect_to_rover_task(),
        rover_blinky_task()
    ).0;
}

static ESP_NOW_MANAGER: Mutex<CriticalSectionRawMutex, Option<EspNowManager<'static>>> = Mutex::new(None);
static ESP_NOW_SENDER: Mutex<CriticalSectionRawMutex, Option<EspNowSender<'static>>> = Mutex::new(None);
static CONNECTED: AtomicBool = AtomicBool::new(false);

async fn get_rover_esp_now_addr() -> Option<[u8; 6]> {
    let connected = CONNECTED.load(Ordering::Relaxed);

    if !connected {
        return None;
    }
    
    let mut manager_unlocked = ESP_NOW_MANAGER.lock().await;
    let m = manager_unlocked.as_mut().unwrap(); 
    return Some(m.fetch_peer(true).unwrap().peer_address);
}

async fn esp_now_send(addr: &[u8; 6], data: &[u8]) -> Result<(), EspNowError> {
    let mut sender_unlocked = ESP_NOW_SENDER.lock().await;
    let s = sender_unlocked.as_mut().unwrap();
    return s.send_async(addr, data).await;
}

async fn esp_now_command_handler(mut receiver: EspNowReceiver<'static>) -> ! {
    loop {
        let r = receiver.receive_async().await;
        let sender_addr = r.info.src_address; 
            
        let mut manager_unlocked = ESP_NOW_MANAGER.lock().await;
        let m = manager_unlocked.as_mut().unwrap(); 

        let connected = CONNECTED.load(Ordering::Relaxed);

        info!("Recv: {:?}", r.data());

        if !connected && r.data().eq(b"strathcuboid-connected") {
            info!("Trying to connect");
            m.add_peer(PeerInfo { 
                interface: esp_radio::esp_now::EspNowWifiInterface::Station, 
                peer_address: sender_addr, 
                lmk: None, 
                channel: None, 
                encrypt: false 
            }).unwrap();

            CONNECTED.store(true, Ordering::Relaxed);

            info!("Connected");
        } else if r.data().eq(b"pong") { // TODO: Handle responses
            info!("Table tennis");
        }
    }
}

async fn connect_to_rover_task() -> ! {
    loop {
        let connected = CONNECTED.load(Ordering::Relaxed);

        if !connected {
            let mut sender_unlocked = ESP_NOW_SENDER.lock().await;
            let s = sender_unlocked.as_mut().unwrap();
            s.send_async(&BROADCAST_ADDRESS, b"strathcuboid-connect").await.unwrap();
            // esp_now_send(&BROADCAST_ADDRESS, b"strathcuboid-connect").await.unwrap();
        } else {
            embassy_time::Timer::after(embassy_time::Duration::from_millis(100)).await;
        }
    }
}

async fn rover_blinky_task() -> ! {
    loop {
        let connected = CONNECTED.load(Ordering::Relaxed);

        if connected {
            info!("Trying to ping rover");
            if let Err(e) = esp_now_send(&get_rover_esp_now_addr().await.unwrap(), b"ping").await {
                error!("Failed to ping rover {:?}", e);
            }
        } else {
            embassy_time::Timer::after(embassy_time::Duration::from_millis(1000)).await;
        }
    }
}
