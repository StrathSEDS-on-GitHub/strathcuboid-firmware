use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_hal::Blocking;
use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::i2c::master::{I2c, Instance};
use esp_hal::time::Rate;
use log::{info, warn};
use pwm_pca9685::{Address, Channel, Pca9685};

use receiver::Movement;

pub static PWM: Mutex<CriticalSectionRawMutex, Option<Pca9685<I2c<'static, Blocking>>>> =
    Mutex::new(None);

pub async fn init_rover(
    i2c: impl Instance + 'static,
    sda: impl PeripheralInput<'static> + PeripheralOutput<'static>,
    sdl: impl PeripheralInput<'static> + PeripheralOutput<'static>,
) {
    let i2c_config = esp_hal::i2c::master::Config::default().with_frequency(Rate::from_hz(1600));

    let i2c_bus = esp_hal::i2c::master::I2c::new(i2c, i2c_config)
        .unwrap()
        .with_sda(sda)
        .with_scl(sdl);

    let mut pwm = Pca9685::new(i2c_bus, Address::default()).unwrap();
    pwm.set_prescale(100).unwrap();
    pwm.enable().unwrap();
    *(PWM.lock()).await = Some(pwm);
}

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

#[derive(PartialEq, Eq)]
enum WheelDirection {
    Forwards,
    Backwards,
    Stop
}

fn is_left_wheel(channel: &Channel) -> bool {
    match channel {
        Channel::C0 => true,
        Channel::C1 => true,
        Channel::C4 => true,
        _ => false
    }
}

fn move_wheel(pwm: &mut Pca9685<I2c<'static, Blocking>>, channel: Channel, dir: WheelDirection) {
    match dir {
        WheelDirection::Forwards if is_left_wheel(&channel) => pwm.set_channel_on_off(channel, 0, 550).unwrap(),
        WheelDirection::Forwards => pwm.set_channel_on_off(channel, 0, 250).unwrap(),
        WheelDirection::Backwards if is_left_wheel(&channel) => pwm.set_channel_on_off(channel, 0, 250).unwrap(),
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
