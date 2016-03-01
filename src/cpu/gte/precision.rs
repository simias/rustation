//! Increased GTE accuracy (sometimes called GTE accuracy)
//! implementation.

/// Type of the increased-precision coordinates. `f32` should be good
/// enough but we could switch to `f64` if the precision is not good
/// enough in certain cases or to some fixed-point type if it provides
/// better performance.
pub type PreciseCoord = f32;

/// In order not to slow things down when GTE accuracy is turned off
/// we use a generic implementation to remove most of the overhead.
pub trait SubpixelPrecision: Clone + Copy {
    /// Create a new "empty" `SubpixelPrecision` instance with no
    /// precise data available.
    fn empty() -> Self;

    /// Create a new SubpixelPrecision
    fn new(x: PreciseCoord, y: PreciseCoord, z: u16) -> Self;

    /// Return the precise vertex coordinates `(x, y, z)` or None if
    /// no precise data is available.
    fn get_precise(&self) -> Option<(PreciseCoord, PreciseCoord, u16)>;
}

/// Dummy implementation for native precision. Does nothing, takes up
/// no space so it should be optimized away if everything goes well.
#[derive(Copy, Clone)]
pub struct NativeVertex;

impl SubpixelPrecision for NativeVertex {
    fn empty() -> NativeVertex {
        NativeVertex
    }

    fn new(_: PreciseCoord, _: PreciseCoord, _: u16) -> NativeVertex {
        NativeVertex::empty()
    }

    fn get_precise(&self) -> Option<(PreciseCoord, PreciseCoord, u16)> {
        None
    }
}

/// Increased precision vertex holding subpixel coordinates or None if
/// we don't have increased precision coordinates available for this
/// memory location.
#[derive(Copy, Clone)]
pub struct PreciseVertex(Option<(f32, f32, u16)>);

impl SubpixelPrecision for PreciseVertex {
    fn empty() -> PreciseVertex {
        PreciseVertex(None)
    }

    fn new(x: f32, y: f32, z: u16) -> PreciseVertex {
        PreciseVertex(Some((x, y, z)))
    }

    fn get_precise(&self) -> Option<(PreciseCoord, PreciseCoord, u16)> {
        self.0
    }
}
