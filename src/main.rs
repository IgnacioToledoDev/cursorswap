// Later tasks consume the domain layer; until then it is intentionally unused.
#![allow(dead_code)]

mod application;
mod domain;

fn main() {
    println!("cambia-cursor {}", env!("CARGO_PKG_VERSION"));
}
