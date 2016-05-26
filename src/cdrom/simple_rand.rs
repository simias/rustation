/// Fast non-cryptographically secure RNG. The implementation is
/// XorShift with a period of (2**31)-1. This is more than sufficient
/// for our use case. This RNG is of course fully deterministic and
/// will always return the same sequence since the seed is fixed.
///
/// See http://www.jstatsoft.org/v08/i14/paper for more details on the
/// algorithm.
///
/// One of the pitfalls of this algorithm is that if the output is
/// used as a raw 32bit random number (without modulo) it'll never
/// return 0.
#[derive(RustcDecodable, RustcEncodable)]
pub struct SimpleRand {
    state: u32,
}

impl SimpleRand {

    /// Create a new FastRand instance using a hardcoded seed
    pub fn new() -> SimpleRand {
        SimpleRand {
            // Arbitrary seed, must be non-0
            state: 1,
        }
    }

    /// Run through one cycle of XorShift and return the internal
    /// pseudo-random state. It will *never* return 0.
    pub fn next(&mut self) -> u32 {
        // The XorShift paper lists a bunch of valid shift triplets, I
        // picked one at random.
        self.state ^= self.state << 6;
        self.state ^= self.state >> 1;
        self.state ^= self.state << 11;

        self.state
    }
}
