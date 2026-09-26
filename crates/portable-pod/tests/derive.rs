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

/// Type parameters with **no `Copy` bound of their own**, which is the common style: bounds go on
/// the impls, not the struct. `Pod: Copy` still has to be discharged, and the field-type bounds do
/// not supply it. This failed to compile before the derive emitted `Self: Copy`. `Queue` and
/// `Guarded` below both happen to declare `T: Copy` inline, which is exactly why they did not
/// catch it.
#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(C)]
struct Unbounded<K, V, const CAP: usize> {
    keys: [K; CAP],
    vals: [V; CAP],
    len: u32,
    _pad: u32,
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

// `assert_layout` is a `const fn`, so an instantiation nothing else reaches is proved by a `const`
// item: a build error, under `cargo check` as well, with no test to run.
// `tests/ui/assert_layout_const_item.rs` is the failing direction.
const _: () = portable_pod::assert_layout::<Ring<3>>();
const _: () = portable_pod::assert_layout::<Ring<0>>();

// `zeroed` is a `const fn` too, so a `const` item or a `const fn` constructor starts from it, with
// the layout proof forced there. `tests/ui/zeroed_const_item.rs` is the failing direction.
const EMPTY_RING: Ring<3> = portable_pod::zeroed();

const fn empty_ring() -> Ring<3> {
    portable_pod::zeroed()
}

#[test]
fn zeroed_in_a_const_is_the_zeroed_value() {
    const FROM_FN: Ring<3> = empty_ring();
    let runtime: Ring<3> = portable_pod::zeroed();
    assert!(bytes_of(&EMPTY_RING).iter().all(|&b| b == 0));
    assert_eq!(bytes_of(&EMPTY_RING), bytes_of(&runtime));
    assert_eq!(bytes_of(&FROM_FN), bytes_of(&runtime));
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

/// Field types spelled with a const-generic block argument. The padding message names every
/// field's type as written, and it used to be the *format string* of the generated `assert!`, so
/// the braces were parsed as a placeholder and the derive failed with `invalid format string` on
/// types that have no padding at all. The other half, that the message still names such a type,
/// braces included, when it does fire, is `tests/ui/internal_padding_braced_type.rs`.
mod braced_field_types {
    use super::*;

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    pub struct Inline<const N: usize> {
        pub a: [u32; N],
    }

    const K: usize = 4;

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    struct BlockArgument {
        inner: Inline<{ K }>,
    }

    /// Generic too, so the braced type also goes through the derive's generic path: a
    /// `Self: Copy` bound, and a proof that runs per instantiation rather than at the definition.
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    struct GenericBlockArgument<T>([T; 2], Inline<{ K * 2 }>);

    #[test]
    fn all_of_these_compile_and_work() {
        assert_eq!(bytes_of(&zeroed::<BlockArgument>()).len(), 16);
        assert_eq!(
            bytes_of(&zeroed::<GenericBlockArgument<u32>>()).len(),
            8 + 32
        );
        assert_eq!(
            bytes_of(&zeroed::<GenericBlockArgument<u64>>()).len(),
            16 + 32
        );
    }
}

#[test]
fn type_parameters_need_no_copy_bound_on_the_struct() {
    // The `Pod: Copy` supertrait is discharged by the derive's own `Self: Copy` predicate.
    let m: Unbounded<u64, u64, 4> = zeroed();
    assert_eq!(bytes_of(&m).len(), 4 * 8 + 4 * 8 + 4 + 4);
    // Still checked per instantiation, and still padding-free for these.
    assert_eq!(bytes_of(&zeroed::<Unbounded<u32, u32, 2>>()).len(), 24);
    assert_eq!(bytes_of(&zeroed::<Unbounded<u16, u16, 2>>()).len(), 16);
    // (`Unbounded<u8, u8, 3>` is *not* in this list: the `u8` arrays leave `len` misaligned, so
    // it has a two-byte gap and its proof correctly refuses to compile.)
}

/// `#[pod(crate = ...)]` roots the expansion somewhere other than `::portable_pod`, so a crate
/// that re-exports the trait can hand its own users a working derive. Here the "re-export" is a
/// module, which is enough to prove the emitted paths follow the attribute rather than the
/// hardcoded default: if any single reference still said `::portable_pod`, this would still
/// compile, so the case that actually pins it is `tests/ui/pod_crate_wrong_path.rs`, where the
/// named path does *not* export `Pod` and every reference must therefore fail.
mod reexport {
    pub use portable_pod::Pod;
}

#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(C)]
#[pod(crate = crate::reexport)]
struct ViaReexport {
    id: u64,
    kind: u32,
    flags: u32,
}

/// The attribute must work for a generic type too: that is the path through the field bounds,
/// the transitive `__LAYOUT_OK` inheritance, and the `Copy` predicates all at once.
#[derive(Clone, Copy, PartialEq, Debug, Pod)]
#[repr(C)]
#[pod(crate = crate::reexport)]
struct GenericViaReexport<T, const N: usize> {
    items: [T; N],
    len: u32,
    _pad: u32,
}

#[test]
fn crate_path_attribute_reroots_the_expansion() {
    let v = ViaReexport {
        id: 1,
        kind: 2,
        flags: 3,
    };
    assert_eq!(bytes_of(&v).len(), 16);
    assert_eq!(read_pod::<ViaReexport>(bytes_of(&v)), Some(v));

    let g: GenericViaReexport<u64, 3> = zeroed();
    assert_eq!(bytes_of(&g).len(), 3 * 8 + 8);
    // The proof is still per instantiation through the re-export.
    assert_eq!(bytes_of(&zeroed::<GenericViaReexport<u32, 5>>()).len(), 28);
}

/// A type parameter reachable only through an **associated-type projection**, with a hand-written
/// `Copy` impl that does not require `T: Copy`.
///
/// This is why the supertrait is discharged as `Self: Copy` rather than as `T: Copy` on each type
/// parameter: the per-parameter form is sufficient but not necessary, and it rejects this. `Tag`
/// is deliberately not `Copy`, yet `Framed<Tag>` is — so it is `Pod`, and a derive that demanded
/// `Tag: Copy` would be inventing a requirement. The typestate/witness shape this uses is common
/// in exactly the wire formats this crate is for.
pub trait Wire: 'static {
    type Word: Pod;
}
pub struct Tag;
impl Wire for Tag {
    type Word = u64;
}

#[derive(Pod)]
#[repr(C)]
pub struct Framed<T: Wire> {
    w: <T as Wire>::Word,
}
impl<T: Wire> Clone for Framed<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: Wire> Copy for Framed<T> {}

#[test]
fn a_parameter_behind_a_projection_need_not_be_copy() {
    let f = Framed::<Tag> {
        w: 0x0102_0304_0506_0708,
    };
    assert_eq!(bytes_of(&f).len(), 8);
}

/// Items declared by a `macro_rules!` macro. Fragments a macro substitutes reach the derive
/// wrapped in invisible (`Delimiter::None`) groups: a `$vis:vis` around the visibility (and around
/// nothing for a private item), a `#[$m:meta]` around the attribute's contents, and a `$t:ty`
/// around the type. `$vis struct` used to fail with "expected a struct definition", and a
/// `#[$m:meta]` carrying the `repr` was not seen as one.
mod macro_declared {
    use super::*;

    macro_rules! named {
        ($(#[$m:meta])* $vis:vis struct $name:ident { $($fvis:vis $f:ident: $t:ty),* $(,)? }) => {
            #[derive(Clone, Copy, PartialEq, Debug, Pod)]
            $(#[$m])*
            $vis struct $name { $($fvis $f: $t),* }
        };
    }

    macro_rules! tuple {
        ($(#[$m:meta])* $vis:vis struct $name:ident ($($fvis:vis $t:ty),* $(,)?);) => {
            #[derive(Clone, Copy, PartialEq, Debug, Pod)]
            $(#[$m])*
            $vis struct $name ($($fvis $t),*);
        };
    }

    named! {
        #[repr(C)]
        pub struct Public { pub a: u32, pub(crate) b: u32, c: u64 }
    }
    named! {
        #[repr(C)]
        pub(crate) struct Crate { pub(super) a: u64 }
    }
    named! {
        #[repr(C, align(8))]
        struct Private { a: u32, b: u32 }
    }
    named! {
        #[repr(C)]
        pub(in crate::macro_declared) struct InPath { pub(in crate::macro_declared) a: u16 }
    }
    named! {
        #[repr(C)]
        #[pod(crate = crate::reexport)]
        pub struct ViaReexport { pub a: u32 }
    }
    tuple! {
        #[repr(C)]
        pub struct PublicTuple(pub u32, pub(crate) u32, u64);
    }
    tuple! {
        #[repr(transparent)]
        struct PrivateTuple(u64);
    }

    #[test]
    fn all_of_these_compile_and_work() {
        let p = Public { a: 1, b: 2, c: 3 };
        assert_eq!(bytes_of(&p).len(), 16);
        assert_eq!(read_pod::<Public>(bytes_of(&p)), Some(p));
        assert_eq!(bytes_of(&Crate { a: 1 }).len(), 8);
        assert_eq!(bytes_of(&Private { a: 1, b: 2 }).len(), 8);
        assert_eq!(bytes_of(&InPath { a: 1 }).len(), 2);
        assert_eq!(bytes_of(&ViaReexport { a: 1 }).len(), 4);
        let t = PublicTuple(1, 2, 3);
        assert_eq!(bytes_of(&t).len(), 16);
        assert_eq!(read_pod::<PublicTuple>(bytes_of(&t)), Some(t));
        assert_eq!(bytes_of(&PrivateTuple(7)).len(), 8);
    }
}

/// `#[pod(size = ...)]` and `#[pod(align = ...)]`. A pin that holds compiles to nothing, so for
/// these types compiling *is* the assertion; the tests below only confirm the pinned values are
/// the real ones. Mismatches, and the diagnostics they produce, are in `tests/ui/*_pin_*.rs`.
mod layout_pins {
    use super::*;

    /// `align(8)` is spelled out because a `u64` is only 4-aligned on 32-bit x86, where a bare
    /// `align = 8` pin would (correctly) fail.
    #[derive(Clone, Copy, Pod)]
    #[repr(C, align(8))]
    #[pod(size = 16, align = 8)]
    struct Record {
        id: u64,
        kind: u32,
        flags: u32,
    }

    /// A pin names constants and composes with `crate`, in either order and across attributes.
    const WORDS: usize = 3;

    #[derive(Clone, Copy, Pod)]
    #[repr(C, align(16))]
    #[pod(align = 16, crate = crate::reexport)]
    #[pod(size = WORDS * 8 + 8)]
    struct Block {
        words: [u64; WORDS],
        tail: u64,
    }

    #[derive(Clone, Copy, Pod)]
    #[repr(transparent)]
    #[pod(size = core::mem::size_of::<u32>(), align = 4)]
    struct Handle(u32);

    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    #[pod(size = 0, align = 1)]
    struct Marker;

    /// A shift is fine once parenthesised; bare, it is refused (`tests/ui/pod_size_bare_shift.rs`).
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    #[pod(size = (1 << 4), align = 4)]
    struct Shifted {
        lanes: [u32; 4],
    }

    /// On a generic type the pin is an identity over the parameters, checked per instantiation.
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    #[pod(size = 4 * N + 4, align = 4)]
    struct Ring<const N: usize> {
        slots: [u32; N],
        len: u32,
    }

    /// A type parameter in the pin, through a turbofish whose comma must not split the argument.
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    #[pod(size = core::mem::size_of::<[T; N]>() + 8)]
    struct Table<T, const N: usize> {
        items: [T; N],
        len: u32,
        _pad: u32,
    }

    /// The same on a generic type, where the pin becomes an assertion rather than a type.
    #[derive(Clone, Copy, Pod)]
    #[repr(C)]
    #[pod(size = (N << 3))]
    struct ShiftedRing<const N: usize> {
        slots: [u64; N],
    }

    macro_rules! pinned {
        ($name:ident, $size:expr) => {
            #[derive(Clone, Copy, Pod)]
            #[repr(C)]
            #[pod(size = $size)]
            struct $name {
                a: u64,
            }
        };
    }
    pinned!(FromMacro, 8);

    #[test]
    fn the_pinned_layouts_are_the_real_ones() {
        assert_eq!(bytes_of(&zeroed::<Record>()).len(), 16);
        assert_eq!(core::mem::align_of::<Record>(), 8);
        assert_eq!(bytes_of(&zeroed::<Block>()).len(), 32);
        assert_eq!(bytes_of(&Handle(7)).len(), 4);
        assert_eq!(bytes_of(&Marker).len(), 0);
        assert_eq!(bytes_of(&zeroed::<Shifted>()).len(), 16);
        assert_eq!(bytes_of(&FromMacro { a: 1 }).len(), 8);
    }

    #[test]
    fn a_generic_pin_holds_for_every_instantiation_that_is_proved() {
        assert_eq!(bytes_of(&zeroed::<Ring<1>>()).len(), 8);
        assert_eq!(bytes_of(&zeroed::<Ring<7>>()).len(), 32);
        assert_eq!(bytes_of(&zeroed::<Table<u64, 3>>()).len(), 32);
        assert_eq!(bytes_of(&zeroed::<Table<u16, 4>>()).len(), 16);
        assert_eq!(bytes_of(&zeroed::<ShiftedRing<2>>()).len(), 16);
    }
}

/// `#[pod(transparent)]`: a one-field newtype whose shape is its field's. The layout proof is the
/// same as without it, so these are behavioural checks; the shapes are in `tests/shape.rs`, and
/// the misuses that must fail in `tests/ui/transparent_*.rs`.
mod transparent {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Debug, Pod)]
    #[repr(transparent)]
    #[pod(transparent)]
    struct Tick(u64);

    /// A named field, and `repr(C)` rather than `repr(transparent)`: one field, so the same layout.
    #[derive(Clone, Copy, PartialEq, Debug, Pod)]
    #[repr(C)]
    #[pod(transparent)]
    struct Meters {
        raw: u32,
    }

    /// Over a struct, with a pin and a re-exported path in the same attribute.
    #[derive(Clone, Copy, PartialEq, Debug, Pod)]
    #[repr(transparent)]
    #[pod(transparent, crate = crate::reexport, size = 8)]
    struct Framed(Header);

    /// Generic: the field is bounded, as for any generic type, and each instantiation proved.
    #[derive(Clone, Copy, PartialEq, Debug, Pod)]
    #[repr(transparent)]
    #[pod(transparent)]
    struct Wrapper<T>(T);

    #[test]
    fn a_transparent_newtype_is_its_field_in_bytes() {
        let t = Tick(0x0102_0304_0506_0708);
        assert_eq!(bytes_of(&t), bytes_of(&t.0));
        assert_eq!(read_pod::<Tick>(bytes_of(&t)), Some(t));

        let m = Meters { raw: 9 };
        assert_eq!(bytes_of(&m), bytes_of(&9u32));

        let f = Framed(Header {
            magic: 1,
            version: 2,
        });
        assert_eq!(read_pod::<Framed>(bytes_of(&f)), Some(f));

        let w = Wrapper(Wrapper(Pair(3, 4)));
        assert_eq!(bytes_of(&w), bytes_of(&Pair(3, 4)));
        assert_eq!(read_pod::<Wrapper<Wrapper<Pair>>>(bytes_of(&w)), Some(w));
    }

    /// A contained padded instantiation is refused through a transparent newtype as through any
    /// other: the inherited proof is the same (`tests/ui/transparent_padding.rs`).
    #[test]
    fn a_transparent_newtype_nests() {
        #[derive(Clone, Copy, PartialEq, Debug, Pod)]
        #[repr(C)]
        struct Outer {
            at: Tick,
            span: Wrapper<Meters>,
            _pad: u32,
        }
        let o = Outer {
            at: Tick(5),
            span: Wrapper(Meters { raw: 6 }),
            _pad: 0,
        };
        assert_eq!(read_pod::<Outer>(bytes_of(&o)), Some(o));
    }
}

/// `SHAPE` is evaluated only where something reads it, for a concrete type as for a generic one. A
/// `shape_with` that panics when evaluated fails a program that reads the shape and no other, and
/// a type whose impl is unconditional (DESIGN.md §3) must not change that by forcing its shape.
mod lazy_shape {
    use super::*;

    const fn unfinished_variant_table() -> u64 {
        panic!("not written yet")
    }

    #[derive(Clone, Copy, PartialEq, Debug, Pod)]
    #[repr(C)]
    #[pod(shape_with = unfinished_variant_table())]
    struct Stored {
        a: u32,
    }

    #[test]
    fn a_shape_nothing_reads_is_never_evaluated() {
        let s = Stored { a: 7 };
        assert_eq!(read_pod::<Stored>(bytes_of(&s)), Some(s));
        assert_eq!(bytes_of(&zeroed::<Stored>()).len(), 4);
    }
}
