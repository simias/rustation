// Rustation - a PlayStation emulator
// Copyright (C) 2015 Lionel Flandrin
//
// This program is free software; you can redistribute it and/or
// modify it under the terms of the GNU General Public License
// as published by the Free Software Foundation; either version 2
// of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program; if not, write to the Free Software
// Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301, USA.

extern crate sdl2;
extern crate gl;
extern crate libc;

mod cpu;
mod memory;
mod gpu;
mod timekeeper;
mod debugger;
mod cdrom;
mod padmemcard;
mod spu;

use std::path::Path;

use gpu::Gpu;
use gpu::opengl::Renderer;
use cpu::Cpu;
use memory::Interconnect;
use memory::bios::Bios;
use debugger::Debugger;
use padmemcard::gamepad;
use padmemcard::gamepad::{Button, ButtonState};
use cdrom::disc::{Disc, Region};

use sdl2::{controller, EventPump};
use sdl2::event::{Event};
use sdl2::keyboard::Keycode;

// Allow dead code so that "cargo test" won't yell at us...
#[allow(dead_code)]
fn main() {
    let argv: Vec<_> = std::env::args().collect();

    if argv.len() < 2 {
        println!("Usage: {} <BIOS-file> [CDROM-bin-file]", argv[0]);
        println!("Recommended BIOS: SCPH1001.BIN");
        return;
    }

    let bios = Bios::new(&Path::new(&argv[1])).unwrap();

    let (disc, video_standard) =
        if argv.len() > 2 {
            let disc_path = &Path::new(&argv[2]);

            match Disc::from_path(disc_path) {
                Ok(disc) => {
                    let region = disc.region();

                    println!("Disc region: {:?}", region);

                    let video_standard =
                        match region {
                            Region::Europe => HardwareType::Pal,
                            Region::NorthAmerica => HardwareType::Ntsc,
                            Region::Japan => HardwareType::Ntsc,
                        };

                    (Some(disc), video_standard)
                }
                Err(e) => {
                    println!("Bad disc: {}", e);
                    return;
                }
            }
        } else {
            // No disc, use region at random. Should probably handle
            // BIOS regions...
            (None, HardwareType::Ntsc)
        };

    // We must initialize SDL before the interconnect is created since
    // it contains the GPU and the GPU needs to create a window
    let sdl_context = sdl2::init().unwrap();

    // When the controller is destroyed SDL2 will stop reporting
    // controller events so we have to make sure to keep it alive
    // until the end of the program.
    let _controller = initialize_sdl2_controllers(&sdl_context);

    let renderer = Renderer::new(&sdl_context);
    let gpu = Gpu::new(renderer, video_standard);
    let inter = Interconnect::new(bios, gpu, disc);
    let mut cpu = Cpu::new(inter);

    let mut debugger = Debugger::new();

    let mut event_pump = sdl_context.event_pump().unwrap();

    loop {
        for _ in 0..1_000_000 {
            cpu.run_next_instruction(&mut debugger);
        }

        match handle_events(&mut event_pump, &mut cpu) {
            Action::None => {},
            Action::Quit => return,
            Action::Debug => debugger.debug(&mut cpu),
        }
    }
}

enum Action {
    None,
    Quit,
    Debug,
}

// Handle SDL events
fn handle_events(event_pump: &mut EventPump, cpu: &mut Cpu) -> Action {
    // Only handle Pad 0 for now.
    let pad = &mut *cpu.pad_profiles()[0];

    for e in event_pump.poll_iter() {
        match e {
            Event::KeyDown { keycode: Some(Keycode::Escape), .. } =>
                return Action::Quit,
            Event::Quit {..} => return Action::Quit,
            Event::KeyDown { keycode: Some(Keycode::Pause), .. } =>
                return Action::Debug,
            Event::KeyDown { keycode: k, .. } =>
                handle_keyboard(pad, k, ButtonState::Pressed),
            Event::KeyUp { keycode: k, .. } =>
                handle_keyboard(pad, k, ButtonState::Released),
            Event::ControllerButtonDown { button: b, .. } =>
                handle_controller(pad, b, ButtonState::Pressed),
            Event::ControllerButtonUp { button: b, .. } =>
                handle_controller(pad, b, ButtonState::Released),
            Event::ControllerAxisMotion { axis, value: val, .. } =>
                update_controller_axis(pad, axis, val),
            _ => (),
        }
    }

    Action::None
}

fn initialize_sdl2_controllers(sdl_context: &sdl2::Sdl) -> Option<controller::GameController> {
    // Attempt to discover and enable a game controller
    let joystick = sdl_context.joystick().unwrap();
    let game_controller = sdl_context.game_controller().unwrap();

    let njoysticks =
        match joystick.num_joysticks() {
            Ok(n)  => n,
            Err(e) => {
                println!("Can't enumarate joysticks: {:?}", e);
                0
            }
        };

    let mut controller = None;

    // For now we just take the first controller we manage to open
    // (if any)
    for id in 0..njoysticks {
        if game_controller.is_game_controller(id) {
            println!("Attempting to open controller {}", id);

            match game_controller.open(id) {
                Ok(c) => {
                    // We managed to find and open a game controller,
                    // exit the loop
                    println!("Successfully opened \"{}\"", c.name());
                    controller = Some(c);
                    break;
                },
                Err(e) => println!("failed: {:?}", e),
            }
        }
    }

    match controller {
        Some(_) => println!("Controller support enabled"),
        None    => println!("No controller found"),
    }

    controller
}

fn handle_keyboard(pad: &mut gamepad::Profile, key: Option<Keycode>, state: ButtonState) {
    let keycode = key.unwrap();
    let button =
        match keycode {
            Keycode::Return => Button::Start,
            Keycode::RShift => Button::Select,
            Keycode::Up => Button::DUp,
            Keycode::Down => Button::DDown,
            Keycode::Left => Button::DLeft,
            Keycode::Right => Button::DRight,
            Keycode::Kp2 => Button::Cross,
            Keycode::Kp4 => Button::Square,
            Keycode::Kp6 => Button::Circle,
            Keycode::Kp8 => Button::Triangle,
            Keycode::Kp7 => Button::L1,
            Keycode::NumLockClear => Button::L2,
            Keycode::Kp9 => Button::R1,
            Keycode::KpMultiply => Button::R2,
            // Unhandled key
            _ => return,
        };

    pad.set_button_state(button, state);
}

fn handle_controller(pad: &mut gamepad::Profile,
                     button: controller::Button,
                     state: ButtonState) {

    // Map the original playstation controller as closely as possible
    // on an XBox 360 controller.
    let button =
        match button {
            controller::Button::Start => Button::Start,
            controller::Button::Back => Button::Select,
            controller::Button::DPadLeft => Button::DLeft,
            controller::Button::DPadRight => Button::DRight,
            controller::Button::DPadUp => Button::DUp,
            controller::Button::DPadDown => Button::DDown,
            controller::Button::A => Button::Cross,
            controller::Button::B => Button::Circle,
            controller::Button::X => Button::Square,
            controller::Button::Y => Button::Triangle,
            controller::Button::LeftShoulder => Button::L1,
            controller::Button::RightShoulder => Button::R1,
            // Unhandled button
            _ => return,
        };

    pad.set_button_state(button, state);
}

fn update_controller_axis(pad: &mut gamepad::Profile,
                          axis: controller::Axis,
                          val: i16) {

    let button =
        match axis {
            controller::Axis::TriggerLeft => Button::L2,
            controller::Axis::TriggerRight => Button::R2,
            // Unhandled axis
            _ => return,
        };

    let state =
        if val < 0x4000 {
            ButtonState::Released
        } else {
            ButtonState::Pressed
        };

    pad.set_button_state(button, state);
}

/// The are a few hardware differences between PAL and NTSC consoles,
/// for instance runs slightly slower on PAL consoles.
#[derive(Clone,Copy)]
pub enum HardwareType {
    Ntsc,
    Pal,
}
