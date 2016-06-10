#[macro_use]
extern crate log;
extern crate shaman;
extern crate cdimage;
extern crate arrayvec;
extern crate rustc_serialize;

#[macro_use]
mod box_array;
#[macro_use]
mod serializer;

pub mod gpu;
pub mod cdrom;
pub mod bios;
pub mod memory;
pub mod cpu;
pub mod shared;
pub mod padmemcard;
pub mod debugger;
pub mod assembler;
pub mod parallel_io;

mod interrupt;
mod timekeeper;
mod spu;
mod mdec;

/// Version of the rustation library set in Cargo.toml
pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

/// Like VERSION but as a `\0`-terminated C string. Useful when you
/// need a static string in C bindings.
pub const VERSION_CSTR: &'static str = concat!(env!("CARGO_PKG_VERSION"), '\0');
