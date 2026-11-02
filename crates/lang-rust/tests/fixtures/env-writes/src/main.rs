use std::env;

fn main() {
    env::set_var("A", "1");
    std::env::set_var("B", "2");
    env::remove_var("C");
    std::env::remove_var("D");
    unsafe {
        env::set_var("E", "3");
    }
}
