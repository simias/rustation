[![Build Status](https://travis-ci.org/simias/rustation.svg)]
(https://travis-ci.org/simias/rustation)

# Rustation PlayStation emulator

![Rustation logo](https://raw.githubusercontent.com/wiki/simias/rustation/images/logo.png)

PlayStation emulator in the Rust programing language.

The focus of this emulator is to write code that's clean, accurate and
hopefully easy to understand. There's no plugin infrastructure, the
emulator is monolithic.

Performance is pretty poor but it should be enough to run close to
realtime on a modern CPU (there's no framelimiting implemented at the
moment).

The current frontend code is built around SDL2. The plan once the
emulator starts to become usable is to turn it into a libretro core so
that it could be played in
[RetroArch](https://github.com/libretro/RetroArch) for instance.

For the time being it can only boot a few games. Crash Bandicoot
(Japanese version) is mostly playable, although I've had random
crashes. Some other games (like Spyro) freeze after or during the
intro.

The GPU rendering is implemented using OpenGL through the [glium
API](https://github.com/tomaka/glium), the idea is to allow things
like increased internal resolution, texture replacement and other
enhancements down the line. Using modern OpenGL it should be possible
to write a flexible yet reasonably accurate renderer. At least that's
the theory, there's quite a lot of work to do before it reaches a
playable state.

If you have any questions, in particular if something in the code is
not clear or properly commented don't hesitate to fill an issue.

I also created a [/psx/ board on 8chan](https://8ch.net/psx/) if you
prefer something less formal to discuss this emulator and all things
PlayStation. We'll see if this turns out to be a good idea...

## Currently implemented (even partially)

![Crash Bandicoot (Japan)]
(https://raw.githubusercontent.com/wiki/simias/rustation/images/crash_bandicoot-level1.png)

* CPU
* Basic GTE support (ported from mednafen PSX)
* Instruction cache
* Interrupts
* Very basic GPU (no semi-transparency or mask bit emulation)
* Timers
* DMA
* Debugger
* CDROM controller (missing many commands)
* Gamepad controller (only digital pad for now)

## Todo list

* Most of the GPU
* MDEC
* SPU
* Memory card
* CPU pipeline emulation
* More accurate timings
* Many, many other things...

## Build

You'll need [Rust and its package manager Cargo]
(https://www.rust-lang.org/),
[SDL2](https://www.libsdl.org/download-2.0.php) and a PlayStation
BIOS. The emulator is mainly tested with BIOS version `SCPH1001` whose
SHA-1 is `10155d8d6e6e832d6ea66db9bc098321fb5e8ebf`.

You should then be able to build the emulator with:

```
cargo build --release
```

Don't forget the `--release` flag in order to turn optimizations
on. Without them the resulting binary will be absurdly slow.

If the build is succesful you can run the emulator using:

```
cargo run --release /path/to/SCPH1001.BIN
```

For Windows check issue [#12](https://github.com/simias/rustation/issues/12).

Use the `Escape` key to exit the emulator, `Pause/Break` to "break" into the
debugger, the emulator will then listen on TCP port `9001` for a GDB
connection.

## Debugger

In order to debug you'll need a GDB targetting
`mipsel-unknown-elf`. Once the emulator is running press the
`Pause/Break` key to trigger the debugger and then connect GDB to it
using (at the gdb command prompt):

`target remote localhost:9001`

GDB might complain about not finding symbols or the boundaries of the
current function but you can ignore that. From then you should be able
to use the familiar [GDB commands]
(https://sourceware.org/gdb/onlinedocs/gdb/) to debug the live
emulator.

A few examples:

```
# Dump the CPU registers
info registers
# Disassemble 20 instructions around PC
disassemble $pc-40,+80
# Display word at address 0x1f801814 (GPU status)
x/x 0x1f801814
# Add code breakpoint at address 0x00004588
break *0x00004588
# Add write watchpoint at address 0x1f801070 (IRQ ack)
watch *0x1f801070
# Step over a single instruction
stepi
# Continue until a break/watchpoint is reached (or Pause/Break is pressed)
continue
```

The debugger support is pretty experimental and quircky but it works
for basic debugging needs.

## Guide

I'm also attempting to document the emulator writing process in a
LaTeX document available in the
[psx-guide repository](https://github.com/simias/psx-guide). It's
generally lagging behind the actual code but I'll try to update it as
often as possible.

## Resources

I try to cite all of my sources in the guide above but I'm mainly
using the priceless [No$ PSX
specifications](http://problemkaputt.de/psx-spx.htm) as well as
[mednafen](http://mednafen.fobby.net/)'s source code when I feel like
cheating.

I also run tests on the real hardware and store them in the
[psx-hardware-tests repository]
(https://github.com/simias/psx-hardware-tests/tree/master/tests).
