// A reference is not position-independent: its bytes are an address. Note this fixture *uses*
// the value. For a generic type an unsatisfiable field bound makes the impl inapplicable rather
// than being an error at the definition, so the diagnostic lands at the use site.
use portable_pod::{bytes_of, Pod};
#[derive(Clone, Copy, Pod)]
#[repr(C)]
struct Borrowed<'a> {
    slot: &'a u32,
}
fn main() {
    let n = 5u32;
    let _ = bytes_of(&Borrowed { slot: &n });
}
