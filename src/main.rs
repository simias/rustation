mod cpu;
mod memory;
mod gpu;

use std::path::Path;

use cpu::Cpu;
use memory::Interconnect;
use memory::bios::Bios;

fn main() {
    let bios = Bios::new(&Path::new("roms/SCPH1001.BIN")).unwrap();

    let inter = Interconnect::new(bios);

    let mut cpu = Cpu::new(inter);

    loop {
        cpu.run_next_instruction();
    }
}
