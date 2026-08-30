use portable_pod::Pod;
#[derive(Clone, Copy, Pod)]
#[repr(C)]
enum Direction {
    North,
    South,
}
fn main() {}
