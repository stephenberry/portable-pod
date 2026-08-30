// Regression for the size-only proof. `a: u32, b: u64` is the adversarial shape: four bytes of
// internal padding at 4..8, yet `size_of` (16) equals the field sum (12) *rounded up to the
// alignment* (8). A size check that allowed that rounding would pass this. The strict equation
// `size_of::<Self>() == sum` does not, which is why the rounding is never permitted.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Padded {
    a: u32,
    b: u64,
}

fn main() {}
