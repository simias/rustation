extern crate sdl2;
extern crate gl;
extern crate libc;

mod cpu;
mod memory;
mod gpu;
mod timekeeper;
mod debugger;

use std::path::Path;

use gpu::Gpu;
use gpu::opengl::Renderer;
use cpu::Cpu;
use memory::Interconnect;
use memory::bios::Bios;
use debugger::Debugger;

use sdl2::event::Event;
use sdl2::keycode::KeyCode;

fn main() {
    let argv: Vec<_> = std::env::args().collect();

    if argv.len() < 2 {
        println!("Usage: {} <BIOS-file>", argv[0]);
        println!("Recommended BIOS: SCPH1001.BIN");
        return;
    }


    let bios = Bios::new(&Path::new(&argv[1])).unwrap();

    // We must initialize SDL before the interconnect is created since
    // it contains the GPU and the GPU needs to create a window
    let sdl_context = sdl2::init(::sdl2::INIT_VIDEO).unwrap();

    let renderer = Renderer::new(&sdl_context);
    let gpu = Gpu::new(renderer, HardwareType::Ntsc);
    let inter = Interconnect::new(bios, gpu);
    let mut cpu = Cpu::new(inter);

    let mut debugger = Debugger::new();

    let mut event_pump = sdl_context.event_pump();

    loop {
        for _ in 0..1_000_000 {
            cpu.run_next_instruction(&mut debugger);
        }

        // See if we should quit
        for e in event_pump.poll_iter() {
            match e {
                Event::KeyDown { keycode: KeyCode::Escape, .. } => return,
                Event::KeyDown { keycode: KeyCode::Pause, .. } =>
                    debugger.debug(&mut cpu),
                Event::Quit {..} => return,
                _ => (),
            }
        }
    }
}

/// The are a few hardware differences between PAL and NTSC consoles,
/// for instance runs slightly slower on PAL consoles.
#[derive(Clone,Copy)]
pub enum HardwareType {
    Ntsc,
    Pal,
}
