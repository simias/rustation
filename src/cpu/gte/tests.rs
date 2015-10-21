use super::Gte;

#[test]
fn gte_lzcr() {
    let expected = [
        (0x00000000, 32),
        (0xffffffff, 32),
        (0x00000001, 31),
        (0x80000000, 1),
        (0x7fffffff, 1),
        (0xdeadbeef, 2),
        (0x000c0ffe, 12),
        (0xfffc0ffe, 14),
    ];

    let mut gte = Gte::new();

    for &(lzcs, lzcr) in &expected {
        gte.set_data(30, lzcs);

        let r = gte.data(31);

        assert!(r == lzcr);
    }
}

#[test]
fn gte_ops() {
    for test in TESTS {
        println!("Test: '{}'", test.desc);
        println!("Command: 0x{:08x}", test.command);

        let mut gte = test.initial.make_gte();

        gte.command(test.command);

        test.result.validate(gte);
    }
}

struct Test {
    /// Test description
    desc: &'static str,
    /// Initial GTE configuration
    initial: Config,
    /// GTE command being executed
    command: u32,
    /// GTE configuration post-command
    result: Config,
}

/// GTE register config: slice of couples `(register_offset,
/// register_value)`. Missing registers are set to 0.
struct Config {
    /// Control registers
    controls: &'static [(u8, u32)],
    /// Data registers
    data: &'static [(u8, u32)],
}

impl Config {
    fn make_gte(&self) -> Gte {
        let mut gte = Gte::new();

        for &(reg, val) in self.controls {
            gte.set_control(reg as u32, val);
        }

        for &(reg, val) in self.data {
            if reg == 15 {
                // Writing to 14 should set this register and writing
                // here will push a new entry onto the XY_FIFO which
                // will change the previous values.
                continue;
            }

            if reg == 28 {
                // This sets the IR1...3 registers MSB but those
                // values should have been set through registers
                // 9...11
                continue;
            }

            if reg == 29 {
                // This register is read only
                continue;
            }

            gte.set_data(reg as u32, val);
        }

        gte
    }

    fn validate(&self, gte: Gte) {
        let mut error_count = 0u32;

        for &(reg, val) in self.controls {
            let v = gte.control(reg as u32);

            if v != val {
                println!("Control register {}: expected 0x{:08x} got 0x{:08x}",
                         reg, val, v);
                error_count += 1;
            }
        }

        for &(reg, val) in self.data {
            let v = gte.data(reg as u32);

            if v != val {
                println!("Data register {}: expected 0x{:08x} got 0x{:08x}",
                         reg, val, v);
                error_count += 1;
            }
        }

        if error_count > 0 {
            panic!("{} registers errors", error_count);
        }
    }
}

/// Reference data generated using tests/gte_commands/main.s in
/// https://github.com/simias/psx-hardware-tests and running it on the
/// real console.
static TESTS: &'static [Test] = &[
    Test {
        desc: "First GTE command used by the SCPH-1001 BIOS",
        initial: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                ],
            data: &[
                (0, 0x00e70119),
                (1, 0xfffffe65),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (31, 0x00000020),
                ],
        },
        command: 0x00080030,
        result: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                (31, 0x00001000),
                ],
            data: &[
                (0, 0x00e70119),
                (1, 0xfffffe65),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (8, 0x00001000),
                (9, 0x0000012b),
                (10, 0xfffffff0),
                (11, 0x000015d9),
                (12, 0x00f40176),
                (13, 0x00f9016b),
                (14, 0x00ed0176),
                (15, 0x00ed0176),
                (17, 0x000015eb),
                (18, 0x000015aa),
                (19, 0x000015d9),
                (24, 0x0106e038),
                (25, 0x0000012b),
                (26, 0xfffffff0),
                (27, 0x000015d9),
                (28, 0x00007c02),
                (29, 0x00007c02),
                (31, 0x00000020),
                ],
        },
    },

    Test {
        desc: "2nd GTE command: RTPT",
        initial: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                (31, 0x00001000),
                ],
            data: &[
                (0, 0x00e70119),
                (1, 0xfffffe65),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (8, 0x00001000),
                (9, 0x0000012b),
                (10, 0xfffffff0),
                (11, 0x000015d9),
                (12, 0x00f40176),
                (13, 0x00f9016b),
                (14, 0x00ed0176),
                (15, 0x00ed0176),
                (17, 0x000015eb),
                (18, 0x000015aa),
                (19, 0x000015d9),
                (24, 0x0106e038),
                (25, 0x0000012b),
                (26, 0xfffffff0),
                (27, 0x000015d9),
                (28, 0x00007c02),
                (29, 0x00007c02),
                (31, 0x00000020),
                ],
        },
        command: 0x00000006,
        result: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                ],
            data: &[
                (0, 0x00e70119),
                (1, 0xfffffe65),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (8, 0x00001000),
                (9, 0x0000012b),
                (10, 0xfffffff0),
                (11, 0x000015d9),
                (12, 0x00f40176),
                (13, 0x00f9016b),
                (14, 0x00ed0176),
                (15, 0x00ed0176),
                (17, 0x000015eb),
                (18, 0x000015aa),
                (19, 0x000015d9),
                (24, 0x0000004d),
                (25, 0x0000012b),
                (26, 0xfffffff0),
                (27, 0x000015d9),
                (28, 0x00007c02),
                (29, 0x00007c02),
                (31, 0x00000020),
                ],
        },
    },

    Test {
        desc: "2nd GTE command: AVSZ3",
        initial: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                ],
            data: &[
                (0, 0x00e70119),
                (1, 0xfffffe65),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (8, 0x00001000),
                (9, 0x0000012b),
                (10, 0xfffffff0),
                (11, 0x000015d9),
                (12, 0x00f40176),
                (13, 0x00f9016b),
                (14, 0x00ed0176),
                (15, 0x00ed0176),
                (17, 0x000015eb),
                (18, 0x000015aa),
                (19, 0x000015d9),
                (24, 0x0000004d),
                (25, 0x0000012b),
                (26, 0xfffffff0),
                (27, 0x000015d9),
                (28, 0x00007c02),
                (29, 0x00007c02),
                (31, 0x00000020),
                ],
        },
        command: 0x0008002d,
        result: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                ],
            data: &[
                (0, 0x00e70119),
                (1, 0xfffffe65),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (7, 0x00000572),
                (8, 0x00001000),
                (9, 0x0000012b),
                (10, 0xfffffff0),
                (11, 0x000015d9),
                (12, 0x00f40176),
                (13, 0x00f9016b),
                (14, 0x00ed0176),
                (15, 0x00ed0176),
                (17, 0x000015eb),
                (18, 0x000015aa),
                (19, 0x000015d9),
                (24, 0x00572786),
                (25, 0x0000012b),
                (26, 0xfffffff0),
                (27, 0x000015d9),
                (28, 0x00007c02),
                (29, 0x00007c02),
                (31, 0x00000020),
                ],
        },
    },

    Test {
        desc: "First NCDS",
        initial: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                ],
            data: &[
                (0, 0x00000b50),
                (1, 0xfffff4b0),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (6, 0x2094a539),
                (7, 0x00000572),
                (8, 0x00001000),
                (9, 0x0000012b),
                (10, 0xfffffff0),
                (11, 0x000015d9),
                (12, 0x00f40176),
                (13, 0x00f9016b),
                (14, 0x00ed0176),
                (15, 0x00ed0176),
                (17, 0x000015eb),
                (18, 0x000015aa),
                (19, 0x000015d9),
                (24, 0x00572786),
                (25, 0x0000012b),
                (26, 0xfffffff0),
                (27, 0x000015d9),
                (28, 0x00007c02),
                (29, 0x00007c02),
                (31, 0x00000020),
                ],
        },
        // GTE_OP GTE_NCDS, lm=1, cv=0, v=0, mx=0, sf=1
        command: 0x00080413,
        result: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                (31, 0x81f00000),
                ],
            data: &[
                (0, 0x00000b50),
                (1, 0xfffff4b0),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (6, 0x2094a539),
                (7, 0x00000572),
                (8, 0x00001000),
                (12, 0x00f40176),
                (13, 0x00f9016b),
                (14, 0x00ed0176),
                (15, 0x00ed0176),
                (17, 0x000015eb),
                (18, 0x000015aa),
                (19, 0x000015d9),
                (22, 0x20000000),
                (24, 0x00572786),
                (25, 0xffffffff),
                (26, 0xffffffff),
                (31, 0x00000020),
                ],
        },
    },
    Test {
        desc: "DPCS random test",
        initial: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                ],
            data: &[
                (0, 0x00000b50),
                (1, 0xfffff4b0),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (6, 0x2094a539),
                (7, 0x00000572),
                (8, 0x00001000),
                (9, 0x0000012b),
                (10, 0xfffffff0),
                (11, 0x000015d9),
                (12, 0x00f40176),
                (13, 0x00f9016b),
                (14, 0x00ed0176),
                (15, 0x00ed0176),
                (17, 0x000015eb),
                (18, 0x000015aa),
                (19, 0x000015d9),
                (24, 0x00572786),
                (25, 0x0000012b),
                (26, 0xfffffff0),
                (27, 0x000015d9),
                (28, 0x00007c02),
                (29, 0x00007c02),
                (31, 0x00000020),
                ],
        },
        command: 0x00080010,
        result: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                ],
            data: &[
                (0, 0x00000b50),
                (1, 0xfffff4b0),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (6, 0x2094a539),
                (7, 0x00000572),
                (8, 0x00001000),
                (12, 0x00f40176),
                (13, 0x00f9016b),
                (14, 0x00ed0176),
                (15, 0x00ed0176),
                (17, 0x000015eb),
                (18, 0x000015aa),
                (19, 0x000015d9),
                (22, 0x20000000),
                (24, 0x00572786),
                (31, 0x00000020),
                ],
        },
    },
    Test {
        desc: "RTPS random test",
        initial: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                ],
            data: &[
                (0, 0x00000b50),
                (1, 0xfffff4b0),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (6, 0x2094a539),
                (8, 0x00001000),
                (31, 0x00000020),
                ],
        },
        command: 0x00080001,
        result: Config {
            controls: &[
                (0, 0x00000ffb),
                (1, 0xffb7ff44),
                (2, 0xf9ca0ebc),
                (3, 0x063700ad),
                (4, 0x00000eb7),
                (6, 0xfffffeac),
                (7, 0x00001700),
                (9, 0x00000fa0),
                (10, 0x0000f060),
                (11, 0x0000f060),
                (13, 0x00000640),
                (14, 0x00000640),
                (15, 0x00000640),
                (16, 0x0bb80fa0),
                (17, 0x0fa00fa0),
                (18, 0x0fa00bb8),
                (19, 0x0bb80fa0),
                (20, 0x00000fa0),
                (24, 0x01400000),
                (25, 0x00f00000),
                (26, 0x00000400),
                (27, 0xfffffec8),
                (28, 0x01400000),
                (29, 0x00000155),
                (30, 0x00000100),
                (31, 0x80004000),
                ],
            data: &[
                (0, 0x00000b50),
                (1, 0xfffff4b0),
                (2, 0x00e700d5),
                (3, 0xfffffe21),
                (4, 0x00b90119),
                (5, 0xfffffe65),
                (6, 0x2094a539),
                (8, 0x00000e08),
                (9, 0x00000bd1),
                (10, 0x000002dc),
                (11, 0x00000d12),
                (14, 0x01d003ff),
                (15, 0x01d003ff),
                (19, 0x00000d12),
                (24, 0x00e08388),
                (25, 0x00000bd1),
                (26, 0x000002dc),
                (27, 0x00000d12),
                (28, 0x000068b7),
                (29, 0x000068b7),
                (31, 0x00000020),
                ],
        },
    },

    ];

