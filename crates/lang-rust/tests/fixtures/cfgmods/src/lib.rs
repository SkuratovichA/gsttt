pub mod util;

#[cfg(unix)]
#[path = "unix.rs"]
mod sys;
#[cfg(windows)]
#[path = "win.rs"]
mod sys;

pub use sys::run;
