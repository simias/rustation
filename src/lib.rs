#[macro_use]
extern crate log;
#[macro_use]
extern crate arrayref;
extern crate shaman;
extern crate cdimage;
extern crate arrayvec;

#[macro_use]
mod box_array;

pub mod gpu;
pub mod cdrom;
pub mod bios;
pub mod memory;
pub mod cpu;
pub mod shared;
pub mod padmemcard;
pub mod debugger;

mod interrupt;
mod timekeeper;
mod spu;

/// Version of the rustation library set in Cargo.toml
pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

/// Like VERSION but as a `\0`-terminated C string. Useful when you
/// need a static string in C bindings.
pub const VERSION_CSTR: &'static str = concat!(env!("CARGO_PKG_VERSION"), '\0');
