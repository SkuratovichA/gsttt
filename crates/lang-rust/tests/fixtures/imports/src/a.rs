pub mod c;
#[cfg(test)]
mod tests;

pub fn helper() -> u32 {
    1
}

pub const SAMPLE: &str = include_str!("../fixtures/a.json");
include!(concat!(env!("OUT_DIR"), "/gen.rs"));
