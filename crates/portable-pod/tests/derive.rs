//! Behavioural tests for `#[derive(Pod)]`. The cases that must *fail* live in `tests/ui/`.

use portable_pod::{Bit, Pod, boxed_zeroed, bytes_of, bytes_of_slice, read_pod, zeroed};

#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(C)]
struct Header {
    magic: u32,
    version: u32,
}

#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(transparent)]
struct Id(u64);

#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(C)]
struct Unit;

#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(C)]
struct Pair(u32, u32);

#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(C)]
struct Nested {
    head: Header,
    id: Id,
    flags: [Bit; 8],
}

#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(C)]
struct Ring<const N: usize> {
    slots: [u32; N],
    len: u32,
}

/// A type parameter, not just const generics.
#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(C)]
struct Queue<T: Copy, const N: usize> {
    items: [T; N],
    head: u32,
    tail: u32,
}

/// Alignment that happens to divide the field sum exactly, so it is still strictly padding-free.
#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(C, align(8))]
struct Aligned {
    a: [u64; 2],
    b: [u64; 2],
}

/// An existing `where` clause must survive, with the derive's bounds appended.
#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(C)]
struct Guarded<T>
where
    T: Copy + core::fmt::Debug,
{
    value: T,
}

#[test]
fn named_struct() {
    let h = Header {
        magic: 0xcafe,
        version: 3,
    };
    assert_eq!(bytes_of(&h).len(), 8);
    assert_eq!(read_pod::<Header>(bytes_of(&h)), Some(h));
}

#[test]
fn transparent_newtype() {
    assert_eq!(bytes_of(&Id(7)).len(), 8);
}

#[test]
fn unit_struct_is_zero_sized() {
    assert_eq!(bytes_of(&Unit).len(), 0);
    assert_eq!(read_pod::<Unit>(&[]), Some(Unit));
}

#[test]
fn tuple_struct() {
    let p = Pair(1, 2);
    assert_eq!(bytes_of(&p).len(), 8);
    assert_eq!(read_pod::<Pair>(bytes_of(&p)), Some(p));
}

#[test]
fn nesting_composes() {
    let n = Nested {
        head: Header {
            magic: 1,
            version: 2,
        },
        id: Id(3),
        flags: [Bit::TRUE; 8],
    };
    assert_eq!(bytes_of(&n).len(), 8 + 8 + 8);
    assert_eq!(read_pod::<Nested>(bytes_of(&n)), Some(n));
}

#[test]
fn generic_instantiations_are_checked_independently() {
    // Each of these is a separate layout proof. All are padding-free: 4N + 4.
    assert_eq!(bytes_of(&zeroed::<Ring<0>>()).len(), 4);
    assert_eq!(bytes_of(&zeroed::<Ring<1>>()).len(), 8);
    assert_eq!(bytes_of(&zeroed::<Ring<7>>()).len(), 32);
    assert_eq!(bytes_of(&zeroed::<Ring<64>>()).len(), 260);
}

#[test]
fn type_parameters_work() {
    let q = Queue::<u16, 4> {
        items: [1, 2, 3, 4],
        head: 0,
        tail: 4,
    };
    assert_eq!(bytes_of(&q).len(), 8 + 4 + 4);
}

#[test]
fn alignment_that_divides_evenly_is_padding_free() {
    assert_eq!(bytes_of(&zeroed::<Aligned>()).len(), 32);
}

#[test]
fn existing_where_clause_survives() {
    assert_eq!(bytes_of(&Guarded { value: 1u32 }).len(), 4);
}

#[test]
fn slices_and_boxes() {
    let ring = boxed_zeroed::<Ring<1024>>();
    assert_eq!(ring.len, 0);
    let hs = [Header {
        magic: 1,
        version: 1,
    }; 3];
    assert_eq!(bytes_of_slice(&hs).len(), 24);
}

/// The layout proof for a generic type is an associated const, which rustc evaluates once per
/// monomorphization. That is what makes every instantiation checked without any of them being
/// named in a test — but it also means the failure is a *post-monomorphization* error, invisible
/// to `cargo check`. This test therefore runs at `cargo test`, where codegen happens.
///
/// Each of these instantiations is a separately proved layout. If any carried padding, building
/// this test would fail.
#[test]
fn layout_proofs_are_post_monomorphization() {
    macro_rules! prove {
        ($($n:literal),*) => {$(
            assert_eq!(bytes_of(&zeroed::<Ring<$n>>()).len(), 4 * $n + 4);
        )*};
    }
    prove!(0, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144);
}

/// Shapes that previously defeated the hand-rolled parser in `parse.rs`. Each of these is valid
/// stable Rust that compiles fine with a plain `#[derive(Clone, Copy)]`; each used to make the
/// derive emit unparsable tokens or reject the item. They are here rather than in `tests/ui/`
/// because the correct behaviour is to *compile*.
mod parser_regressions {
    use super::*;

    pub trait Two {
        type A;
        type B;
    }
    impl Two for u32 {
        type A = fn() -> u8;
        type B = u32;
    }

    /// A `->` inside a bound, followed by another associated-type binding. The `>` closing the
    /// arrow used to underflow the angle-depth counter, so `B = u32` was mistaken for a
    /// top-level default and the parameter was truncated mid-bound (dropping `+ Copy` too).
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    struct ArrowInBound<T: Two<A = fn() -> u8, B = u32> + Copy>(T);

    #[derive(Clone, Copy)]
    #[repr(C)]
    struct Arr<const N: usize>([u8; N]);

    /// A braced const argument in a `where` clause. Angle brackets are not token-tree groups, so
    /// the clause scanner used to stop at `{ 2 * 2 }`, believing it was the struct body.
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    struct BracedWhereTuple<T: Copy>(T)
    where
        Arr<{ 2 * 2 }>: Copy;

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    struct BracedWhereNamed<T: Copy>
    where
        Arr<{ 2 * 2 }>: Copy,
    {
        a: T,
    }

    /// A default whose `=` abuts a punct, so the token is `Joint`. A spacing test read that as
    /// "not a default" and let it through into the impl generics, which rustc rejects.
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    struct JointDefault<T: Copy = *const u32>(T);

    /// An attribute on a generic parameter. `cfg(all())` is the always-true cfg, which is the
    /// simplest way to put a real attribute there; clippy would rather it were written without
    /// the `all()`, but then it would not be the shape under test.
    #[allow(clippy::non_minimal_cfg)]
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    struct AttrOnParam<#[cfg(all())] T: Copy>(T);

    #[test]
    fn all_of_these_compile_and_work() {
        assert_eq!(bytes_of(&ArrowInBound::<u32>(7)).len(), 4);
        assert_eq!(bytes_of(&BracedWhereTuple::<u16>(1)).len(), 2);
        assert_eq!(bytes_of(&BracedWhereNamed::<u64> { a: 1 }).len(), 8);
        assert_eq!(bytes_of(&JointDefault::<u32>(3)).len(), 4);
        assert_eq!(bytes_of(&AttrOnParam::<u8>(1)).len(), 1);
    }
}
