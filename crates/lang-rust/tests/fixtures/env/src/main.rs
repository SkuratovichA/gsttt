use clap::Parser;
use std::env;

#[derive(Parser)]
struct Args {
    #[arg(long, env = "E")]
    listen: String,
}

fn main() {
    let a = env::var("A");
    let b = std::env::var_os("B");
    let c = env!("C");
    let d = option_env!("D");
    let name = format!("{}_URL", "F");
    let computed = env::var(&name);
    let _ = (a, b, c, d, computed, Args::parse());
}
