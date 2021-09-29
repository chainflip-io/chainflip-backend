//! Substrate Node Template CLI library.
#![warn(missing_docs)]

mod chain_spec;
#[macro_use]
#[rustfmt::skip]
mod service;
mod cli;
#[rustfmt::skip]
mod command;

fn main() -> sc_cli::Result<()> {
	command::run()
}
