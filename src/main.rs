mod application;
mod bootstrap;
mod domain;
mod infrastructure;
mod presentation;

use std::process::ExitCode;

fn main() -> ExitCode {
    bootstrap::run(std::env::args_os(), |key| std::env::var_os(key))
}
