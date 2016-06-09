//! Helper macros for serialization purposes.

/// Create a wrapper type around a function pointer in order to be
/// able to serialize it. The new type implements Deref an DerefMut
/// for convenience. In order to be able to encode and decode the
/// function this macro needs to be given the *exhaustive* list of all
/// the methods that can be stored it in, otherwise the
/// encoding/decoding will fail for unknown functions:
///
/// ```
/// # #[macro_use] extern crate rustation;
/// # #[macro_use] extern crate rustc_serialize;
/// # fn main() {
/// callback!(
///    struct MyHandler(fn(u32) -> bool) {
///        foo,
///        bar,
///        baz,
///    });
///
///  fn foo(_: u32) -> bool { true }
///  fn bar(_: u32) -> bool { false }
///  fn baz(v: u32) -> bool { v == 0 }
/// # }
/// ```
#[macro_export]
macro_rules! callback {
    (struct $st:ident ($proto:ty) {
        $($ptr:expr),+$(,)*
    }) => (
        #[derive(Copy)]
        struct $st($proto);
       
        impl Clone for $st {
            fn clone(&self) -> Self {
                *self
            }
        }

        // Implement Deref to make accessing the underlying function
        // pointer more convenient
        impl ::std::ops::Deref for $st {
            type Target = $proto;

            fn deref(&self) -> &$proto {
                &self.0
            }
        }

        // Implement DerefMut to make setting the underlying function
        // pointer more convenient
        impl ::std::ops::DerefMut for $st {
            fn deref_mut(&mut self) -> &mut $proto {
                &mut self.0
            }
        }

        impl ::rustc_serialize::Encodable for $st {
            fn encode<S>(&self, s: &mut S) -> Result<(), S::Error>
                where S: ::rustc_serialize::Encoder {
                let address = self.0 as usize;

                let lut = [
                    $(($ptr as usize, stringify!($ptr))),+,
                ];

                for &(a, n) in lut.iter() {
                    if address == a {
                        return s.emit_str(n)
                    }
                }

                panic!("Unexpected method pointer {:x}", address);
            }
        }

        impl ::rustc_serialize::Decodable for $st {
            fn decode<D>(d: &mut D) -> Result<$st, D::Error>
                where D: ::rustc_serialize::Decoder {

                let symbol = try!(d.read_str());

                let lut = [
                    $(($ptr as $proto, stringify!($ptr))),+,
                ];

                for &(f, n) in lut.iter() {
                    if symbol == n {
                        return Ok($st(f));
                    }
                }

                Err(d.error("Unknown callback"))
            }
        }
    );
}

/// Create a wrapper type around an array in order to be able to
/// serialize it. The new type implements Deref an DerefMut for
/// convenience. The element type must implement
/// `std::default::Default`:
///
/// ```
/// # #[macro_use] extern crate rustation;
/// # #[macro_use] extern crate rustc_serialize;
/// # fn main() {
/// buffer!(struct MyBuffer([u8; 1024]));
///
/// let mut buf = MyBuffer::new();
/// assert!(buf[55] == 0);
/// buf[55] += 1;
/// assert!(buf[55] == 1);
/// # }
/// ```
#[macro_export]
macro_rules! buffer {
    (struct $st:ident ([$elem: ty; $len: expr])) => (
        struct $st([$elem; $len]);

        impl $st {
            /// Build a new $st using the `Default` constructor
            fn new() -> $st {
                ::std::default::Default::default()
            }
        }

        impl ::std::default::Default for $st {
            fn default() -> $st {
                $st([::std::default::Default::default(); $len])
            }
        }

        // Implement Deref to make accessing the underlying function
        // pointer more convenient
        impl ::std::ops::Deref for $st {
            type Target = [$elem; $len];

            fn deref(&self) -> &[$elem; $len] {
                &self.0
            }
        }

        // Implement DerefMut to make setting the underlying function
        // pointer more convenient
        impl ::std::ops::DerefMut for $st {
            fn deref_mut(&mut self) -> &mut [$elem; $len] {
                &mut self.0
            }
        }

        impl ::rustc_serialize::Encodable for $st {
            fn encode<S>(&self, s: &mut S) -> Result<(), S::Error>
                where S: ::rustc_serialize::Encoder {

                s.emit_seq($len, |s| {
                    for (i, b) in self.0.iter().enumerate() {
                        try!(s.emit_seq_elt(i, |s| b.encode(s)));
                    }
                    Ok(())
                })
            }
        }

        impl ::rustc_serialize::Decodable for $st {
            fn decode<D>(d: &mut D) -> Result<$st, D::Error>
                where D: ::rustc_serialize::Decoder {

                use ::rustc_serialize::Decodable;

                d.read_seq(|d, len| {
                    if len != $len {
                        return Err(d.error("wrong buffer length"));
                    }

                    let mut b = $st::new();

                    for (i, b) in b.0.iter_mut().enumerate() {
                        *b = try!(d.read_seq_elt(i, Decodable::decode))
                    }

                    Ok(b)
                })
            }
        }
    );
}
