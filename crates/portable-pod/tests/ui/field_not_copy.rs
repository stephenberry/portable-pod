use portable_pod::Pod;
#[derive(Pod)]
#[repr(C)]
struct Owned {
    name: String,
}
fn main() {}
