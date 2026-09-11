use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_hal::Blocking;
use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::i2c::master::{I2c, Instance};
use esp_hal::time::Rate;
use pwm_pca9685::{Address, Channel, Pca9685};

pub static PWM: Mutex<CriticalSectionRawMutex, Option<Pca9685<I2c<'static, Blocking>>>> = Mutex::new(None);

pub async fn init_rover(
    i2c: impl Instance + 'static, 
    sda: impl PeripheralInput<'static> + PeripheralOutput<'static>,
    sdl: impl PeripheralInput<'static> + PeripheralOutput<'static>
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

pub async fn forwards() {
    if let Some(pwm) = PWM.lock().await.as_mut() {
        pwm.set_channel_on_off(Channel::C1, 0, 250).unwrap();
        pwm.set_channel_on_off(Channel::C2, 0, 550).unwrap();
        pwm.set_channel_on_off(Channel::C0, 0, 250).unwrap();
        pwm.set_channel_on_off(Channel::C3, 0, 550).unwrap();
    }
}

pub async fn backwards() {
    if let Some(pwm) = PWM.lock().await.as_mut() {
        pwm.set_channel_on_off(Channel::C1, 0, 550).unwrap();
        pwm.set_channel_on_off(Channel::C2, 0, 250).unwrap();
        pwm.set_channel_on_off(Channel::C0, 0, 550).unwrap();
        pwm.set_channel_on_off(Channel::C3, 0, 250).unwrap();
    }
}

pub async fn left() {
    if let Some(pwm) = PWM.lock().await.as_mut() {
        pwm.set_channel_on_off(Channel::C1, 0, 250).unwrap();
        pwm.set_channel_on_off(Channel::C2, 0, 250).unwrap();
        pwm.set_channel_on_off(Channel::C0, 0, 250).unwrap();
        pwm.set_channel_on_off(Channel::C3, 0, 250).unwrap();
    }
}

pub async fn right() {
    if let Some(pwm) = PWM.lock().await.as_mut() {
        pwm.set_channel_on_off(Channel::C1, 0, 550).unwrap();
        pwm.set_channel_on_off(Channel::C2, 0, 550).unwrap();
        pwm.set_channel_on_off(Channel::C0, 0, 550).unwrap();
        pwm.set_channel_on_off(Channel::C3, 0, 550).unwrap();
    }
}

pub async fn stop() {
    if let Some(pwm) = PWM.lock().await.as_mut() {
        pwm.set_channel_on_off(Channel::C1, 0, 0).unwrap();
        pwm.set_channel_on_off(Channel::C2, 0, 0).unwrap();
        pwm.set_channel_on_off(Channel::C0, 0, 0).unwrap();
        pwm.set_channel_on_off(Channel::C3, 0, 0).unwrap();
    }
}
