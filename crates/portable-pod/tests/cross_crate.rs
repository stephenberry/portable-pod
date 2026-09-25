//! Two fields whose types are spelled alike but are different types.
//!
//! `cross_crate_fixture::record!` puts a field of type `$crate::Header` (its own crate's `Header`)
//! into the struct it derives. The `mine!` macro here passes a second field, also spelled
//! `$crate::Header`, which resolves to *this* crate's `Header`. Both print as `$crate :: Header`,
//! and the derive used to merge fields by that spelling: the second field was never bounded `Pod`
//! (unsound: `tests/ui/cross_crate_non_pod_field.rs` is that half) and its shape was taken from the
//! first. This is the shape half.

use portable_pod::Pod;
use portable_pod::shape::Fold;

#[derive(Clone, Copy, Pod)]
#[repr(C)]
pub struct Header {
    pub tag: u8,
}

macro_rules! mine {
    () => {
        cross_crate_fixture::record!(Out { pub body: $crate::Header, });
    };
}
mine!();

#[test]
fn each_field_has_its_own_types_shape() {
    let theirs = cross_crate_fixture::Header::SHAPE;
    let ours = Header::SHAPE;
    assert_ne!(theirs, ours);
    assert_eq!(
        Out::SHAPE,
        Fold::new()
            .field("head", theirs)
            .field("body", ours)
            .finish(2)
    );
    assert_ne!(
        Out::SHAPE,
        Fold::new()
            .field("head", theirs)
            .field("body", theirs)
            .finish(2),
        "the second field's shape was taken from the first"
    );
}

#[test]
fn it_round_trips() {
    let out = Out {
        head: cross_crate_fixture::Header { id: 1 },
        body: Header { tag: 2 },
    };
    let back = cross_crate_fixture::read_pod::<Out>(portable_pod::bytes_of(&out)).unwrap();
    assert_eq!((back.head.id, back.body.tag), (1, 2));
}
