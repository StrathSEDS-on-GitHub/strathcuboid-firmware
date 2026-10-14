#![no_std]

use core::str::FromStr;
#[derive(Debug)]
pub enum Movement {
    Forward,
    Backward,
    Left,
    Right,
    Stop,
}

impl FromStr for Movement {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "forward" => Result::Ok(Self::Forward),
            "backward" => Result::Ok(Self::Backward),
            "left" => Result::Ok(Self::Left),
            "right" => Result::Ok(Self::Right),
            "stop" => Result::Ok(Self::Stop),
            _ => Result::Err(()),
        }
    }
}
