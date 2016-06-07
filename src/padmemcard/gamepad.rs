use rustc_serialize::{Decodable, Encodable, Decoder, Encoder};

pub struct GamePad {
    /// Gamepad profile. *Not* stored in the savestate.
    profile: Box<Profile>,
    /// Counter keeping track of the current position in the reply
    /// sequence
    seq:    u8,
    /// False if the pad is done processing the current command
    active: bool,
}

impl GamePad {
    /// Create a new disconnected GamePad
    pub fn disconnected() -> GamePad {
        GamePad {
            seq: 0,
            active: true,
            profile: Box::new(DisconnectedProfile),
        }
    }

    /// Called when the "select" line goes down.
    pub fn select(&mut self) {
        // Prepare for incomming command
        self.active = true;
        self.seq = 0;
    }

    /// The 2nd return value is the response byte. The 2nd return
    /// value is true if the gamepad issues a DSR pulse after the byte
    /// is read to notify the controller that more data can be read.
    pub fn send_command(&mut self, cmd: u8) -> (u8, bool) {
        if !self.active {
            return (0xff, false);
        }

        let (resp, dsr) = self.profile.handle_command(self.seq, cmd);

        // If we're not asserting DSR it either means that we've
        // encountered an error or that we have nothing else to
        // reply. In either case we won't be handling any more command
        // bytes in this transaction.
        self.active = dsr;

        self.seq += 1;

        (resp, dsr)
    }

    /// Return a mutable reference to the underlying gamepad Profile
    pub fn profile_mut(&mut self) -> &mut Profile {
        &mut *self.profile
    }

    pub fn set_profile(&mut self, profile: Box<Profile>) {
        self.profile = profile
    }
}

impl Encodable for GamePad {
    fn encode<S: Encoder>(&self, s: &mut S) -> Result<(), S::Error> {

        // We don't store the Profile in the serialized data, we'll
        // let the frontend reset it
        s.emit_struct("GamePad", 2, |s| {

            try!(s.emit_struct_field("seq", 0,
                                     |s| self.seq.encode(s)));
            try!(s.emit_struct_field("active", 1,
                                     |s| self.active.encode(s)));
            
            Ok(())
        })
    }
}

impl Decodable for GamePad {
    fn decode<D: Decoder>(d: &mut D) -> Result<GamePad, D::Error> {

        d.read_struct("GamePad", 2, |d| {
            let mut pad = GamePad::disconnected();

            pad.seq =
                try!(d.read_struct_field("seq", 0, Decodable::decode));

            pad.active =
                try!(d.read_struct_field("active", 1, Decodable::decode));

            Ok(pad)
        })
    }
}

/// Digital buttons on a PlayStation controller. The value assigned to
/// each button is the bit position in the 16bit word returned in the
/// serial protocol
#[derive(Clone,Copy,Debug)]
pub enum Button {
    Select = 0,
    Start = 3,
    DUp = 4,
    DRight = 5,
    DDown = 6,
    DLeft = 7,
    L2 = 8,
    R2 = 9,
    L1 = 10,
    R1 = 11,
    Triangle = 12,
    Circle = 13,
    Cross = 14,
    Square = 15,
}

#[derive(Clone,Copy,Debug)]
pub enum ButtonState {
    Pressed,
    Released,
}

/// Trait used to abstract away the various controller types.
pub trait Profile {
    /// Handle a command byte sent by the console. `seq` is the byte
    /// position in the current command starting with `1` since byte
    /// `0` is expected to always be `0x01` when addressing a
    /// controller and is handled at the top level.
    ///
    /// Returns a pair `(response, dsr)`. If DSR is false the
    /// subsequent command bytes will be ignored for the current
    /// transaction.
    fn handle_command(&mut self, seq: u8, cmd: u8) -> (u8, bool);

    /// Set a button's state. The function can be called several time
    /// in a row with the same button and the same state, it should be
    /// idempotent.
    fn set_button_state(&mut self, button: Button, state: ButtonState);
}

/// Dummy profile emulating an empty pad slot
pub struct DisconnectedProfile;

impl Profile for DisconnectedProfile {
    fn handle_command(&mut self, _: u8, _: u8) -> (u8, bool) {
        // The bus is open, no response
        (0xff, false)
    }

    fn set_button_state(&mut self, _: Button, _: ButtonState) {
    }
}

/// SCPH-1080: Digital gamepad.
/// Full state is only two bytes since we only need one bit per
/// button.
pub struct DigitalProfile(u16);

impl DigitalProfile {
    pub fn new() -> DigitalProfile {
        DigitalProfile(0xffff)
    }
}

impl Profile for DigitalProfile {
    fn handle_command(&mut self, seq: u8, cmd: u8) -> (u8, bool) {
        match seq {
            // First byte should be 0x01 if the command targets
            // the controller
            0 => (0xff, (cmd == 0x01)),
            // Digital gamepad only supports command 0x42: read
            // buttons.
            // Response 0x41: we're a digital PSX controller
            1 => (0x41, (cmd == 0x42)),
            // From then on the command byte is ignored.
            // Response 0x5a: 2nd controller ID byte
            2 => (0x5a, true),
            // First button state byte: direction cross, start and
            // select.
            3 => (self.0 as u8, true),
            // 2nd button state byte: shoulder buttons and "shape"
            // buttons. We don't asert DSR for the last byte.
            4 => ((self.0 >> 8) as u8, false),
            // Shouldn't be reached
            _ => (0xff, false),
        }
    }

    fn set_button_state(&mut self, button: Button, state: ButtonState) {
        let s = self.0;

        let mask = 1 << (button as usize);

        self.0 =
            match state {
                ButtonState::Pressed  => s & !mask,
                ButtonState::Released => s | mask,
            };
    }
}
