//! Allocating fixed size arrays on the heap without unsafe code is
//! pretty hard while the box syntax is unstable. As a workaround we
//! implement a macro which does just that.

/// A macro similar to `vec![$elem; $size]` which returns a boxed
/// array.
///
/// ```rustc
///     let _: Box<[u8; 1024]> = box_array![0; 1024];
/// ```
macro_rules! box_array {
    ($val:expr ; $len:expr) => ({
        // Use a generic function so that the pointer cast remains
        // type-safe
        fn vec_to_boxed_array<T>(vec: Vec<T>) -> Box<[T; $len]> {
            let boxed_slice = vec.into_boxed_slice();

            let ptr =
                ::std::boxed::Box::into_raw(boxed_slice) as *mut [T; $len];

            unsafe { Box::from_raw(ptr) }
        }

        vec_to_boxed_array(vec![$val; $len])
    })
}
