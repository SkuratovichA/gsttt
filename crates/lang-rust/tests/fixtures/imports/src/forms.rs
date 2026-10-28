use crate::a::helper;
use crate::a::{self, c::Thing as Renamed};
pub use crate::a::SAMPLE;
use crate::a::c::*;
pub use crate::a::c::*;
extern crate serde as sd;
mod local;

fn call() -> u32 {
    crate::a::helper() + self::local::go()
}

const DATA: &str = include_str!("data.txt");
const BYTES: &[u8] = include_bytes!("data.txt");
include!("snippet.rs");
