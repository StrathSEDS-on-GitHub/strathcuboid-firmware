use core::sync::atomic::{Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_hal::Blocking;
use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::i2c::master::{I2c, Instance};
use esp_hal::time::Rate;
use log::info;
use pwm_pca9685::{Address, Channel, Pca9685};
use web_rover::{Emote, Movement};

use crate::web::SHARED_STOP;

pub static PWM: Mutex<CriticalSectionRawMutex, Option<Pca9685<I2c<'static, Blocking>>>> =
    Mutex::new(None);

pub async fn init_rover(
    i2c: impl Instance + 'static,
    sda: impl PeripheralInput<'static> + PeripheralOutput<'static>,
    sdl: impl PeripheralInput<'static> + PeripheralOutput<'static>,
) -> Result<(), &'static str> {
    let i2c_config = esp_hal::i2c::master::Config::default().with_frequency(Rate::from_hz(1600));

    let i2c_bus = esp_hal::i2c::master::I2c::new(i2c, i2c_config)
        .unwrap()
        .with_sda(sda)
        .with_scl(sdl);

    let mut pwm =
        Pca9685::new(i2c_bus, Address::default()).map_err(|_| "Failed to initialize PCA9685")?;
    pwm.set_prescale(100)
        .map_err(|_| "Failed to set prescale value")?;
    pwm.enable().map_err(|_| "Failed to enable PCA9685")?;

    *(PWM.lock()).await = Some(pwm);

    Ok(())
}

/*
Wheels:
C0: Front Left
C1: Back Right
C2: Back Left
C3: Front Right
C4: Middle Left
C5: Middle Right
*/


pub async fn move_rover(movement: Movement) {
    info!("move to the {:?}", movement);
    match movement {
        Movement::Forward => forwards().await,
        Movement::Backward => backwards().await,
        Movement::Left => left().await,
        Movement::Right => right().await,
        Movement::Stop => stop().await,
    }
}

#[embassy_executor::task]
pub async fn emote_rover(emote: Emote) {
    info!("emote {:?}", emote);
    match emote {
        Emote::Shuffle => shuffle().await
    }
}

#[derive(PartialEq, Eq)]
enum WheelDirection {
    Forwards,
    Backwards,
    Stop,
}

fn is_left_wheel(channel: &Channel) -> bool {
    match channel {
        Channel::C0 => true,
        Channel::C1 => true,
        Channel::C4 => true,
        _ => false,
    }
}

fn move_wheel(pwm: &mut Pca9685<I2c<'static, Blocking>>, channel: Channel, dir: WheelDirection) {
    match dir {
        WheelDirection::Forwards if is_left_wheel(&channel) => {
            pwm.set_channel_on_off(channel, 0, 550).unwrap()
        }
        WheelDirection::Forwards => pwm.set_channel_on_off(channel, 0, 250).unwrap(),
        WheelDirection::Backwards if is_left_wheel(&channel) => {
            pwm.set_channel_on_off(channel, 0, 250).unwrap()
        }
        WheelDirection::Backwards => pwm.set_channel_on_off(channel, 0, 550).unwrap(),
        WheelDirection::Stop => pwm.set_channel_on_off(channel, 0, 0).unwrap(),
    }
}

async fn forwards() {
    let mut pwm_guard = PWM.lock().await;
    let pwm = pwm_guard.as_mut().unwrap();

    move_wheel(pwm, Channel::C0, WheelDirection::Forwards);
    move_wheel(pwm, Channel::C1, WheelDirection::Forwards);
    move_wheel(pwm, Channel::C2, WheelDirection::Forwards);
    move_wheel(pwm, Channel::C3, WheelDirection::Forwards);
    move_wheel(pwm, Channel::C4, WheelDirection::Forwards);
    move_wheel(pwm, Channel::C5, WheelDirection::Forwards);
}

async fn backwards() {
    let mut pwm_guard = PWM.lock().await;
    let pwm = pwm_guard.as_mut().unwrap();

    move_wheel(pwm, Channel::C0, WheelDirection::Backwards);
    move_wheel(pwm, Channel::C1, WheelDirection::Backwards);
    move_wheel(pwm, Channel::C2, WheelDirection::Backwards);
    move_wheel(pwm, Channel::C3, WheelDirection::Backwards);
    move_wheel(pwm, Channel::C4, WheelDirection::Backwards);
    move_wheel(pwm, Channel::C5, WheelDirection::Backwards);
}

async fn left() {
    let mut pwm_guard = PWM.lock().await;
    let pwm = pwm_guard.as_mut().unwrap();

    move_wheel(pwm, Channel::C0, WheelDirection::Backwards);
    move_wheel(pwm, Channel::C1, WheelDirection::Backwards);
    move_wheel(pwm, Channel::C2, WheelDirection::Forwards);
    move_wheel(pwm, Channel::C3, WheelDirection::Forwards);
    move_wheel(pwm, Channel::C4, WheelDirection::Backwards);
    move_wheel(pwm, Channel::C5, WheelDirection::Forwards);
}

async fn right() {
    let mut pwm_guard = PWM.lock().await;
    let pwm = pwm_guard.as_mut().unwrap();

    move_wheel(pwm, Channel::C0, WheelDirection::Forwards);
    move_wheel(pwm, Channel::C1, WheelDirection::Forwards);
    move_wheel(pwm, Channel::C2, WheelDirection::Backwards);
    move_wheel(pwm, Channel::C3, WheelDirection::Backwards);
    move_wheel(pwm, Channel::C4, WheelDirection::Forwards);
    move_wheel(pwm, Channel::C5, WheelDirection::Backwards);
}

async fn stop() {
    let mut pwm_guard = PWM.lock().await;
    let pwm = pwm_guard.as_mut().unwrap();

    move_wheel(pwm, Channel::C0, WheelDirection::Stop);
    move_wheel(pwm, Channel::C1, WheelDirection::Stop);
    move_wheel(pwm, Channel::C2, WheelDirection::Stop);
    move_wheel(pwm, Channel::C3, WheelDirection::Stop);
    move_wheel(pwm, Channel::C4, WheelDirection::Stop);
    move_wheel(pwm, Channel::C5, WheelDirection::Stop);
}

async fn shuffle() {
    for _i in 0..10 {
        let shared_bool = SHARED_STOP.load(Ordering::Relaxed);
        if !shared_bool {
            break;
        }
        left().await;
        Timer::after(Duration::from_hz(2)).await;
        let shared_bool = SHARED_STOP.load(Ordering::Relaxed);
        if !shared_bool {
            break;
        }
        right().await;
        Timer::after(Duration::from_hz(2)).await;
    }
}
