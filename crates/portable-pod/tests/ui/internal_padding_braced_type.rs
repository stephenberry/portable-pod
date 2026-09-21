// Regression: a field type spelled with a const-generic block argument. The message lists each
// field's type as written, and it used to be the generated `assert!`'s format string, so `{ K }`
// was read as a placeholder and the derive failed with `invalid format string` instead of
// reporting the padding. The listing must still name the braced type, braces included.
use portable_pod::Pod;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Inline<const N: usize> {
    a: [u8; N],
}

const K: usize = 3;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Padded {
    head: Inline<{ K }>,
    tail: u32,
}

fn main() {}
