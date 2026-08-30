use alloc::alloc::{Layout, alloc_zeroed, handle_alloc_error};
use alloc::boxed::Box;

use crate::Pod;

/// A heap-allocated, freshly zeroed `T`, built in place.
///
/// The point is the *in place*: there is no `T`-sized stack temporary, so this works for types
/// far larger than the stack can hold. `Box::new(zeroed::<T>())` constructs on the stack first
/// and then copies, which overflows for a multi-megabyte `T` in debug builds.
///
/// ```
/// # use portable_pod::{Pod, boxed_zeroed};
/// #[derive(Clone, Copy, Pod)]
/// #[repr(C)]
/// struct Big { data: [u64; 1024] }
///
/// let b = boxed_zeroed::<Big>();
/// assert_eq!(b.data[0], 0);
/// ```
#[inline]
#[must_use]
pub fn boxed_zeroed<T: Pod>() -> Box<T> {
    crate::prove_layout::<T>();
    let layout = Layout::new::<T>();
    let ptr = if layout.size() == 0 {
        // `alloc_zeroed` forbids a zero-sized layout, and `Box` represents a zero-sized value as
        // a dangling, correctly-aligned pointer. Build it that way rather than via
        // `Box::new(zeroed::<T>())`: a debug build reserves a stack slot for every local, even
        // one in a branch it will never take, so naming a `T` value here would put a `T` on the
        // stack for *every* instantiation -- defeating the whole purpose of this function, which
        // exists to allocate types far larger than the stack can hold. (Caught by
        // `large_allocation_does_not_touch_the_stack`, which overflowed on an 8 MB type.)
        core::ptr::NonNull::<T>::dangling().as_ptr()
    } else {
        // SAFETY: `layout` is non-zero-sized, as `alloc_zeroed` requires.
        let p = unsafe { alloc_zeroed(layout) }.cast::<T>();
        if p.is_null() {
            handle_alloc_error(layout);
        }
        p
    };
    // SAFETY: `ptr` is non-null, correctly aligned, and uniquely owned. For a sized `T` it is a
    // live allocation of exactly `Layout::new::<T>()` filled with zeroes; for a zero-sized `T` it
    // is the dangling pointer `Box` itself uses. Either way `T: Pod` makes the all-zero bit
    // pattern a valid `T` (clause 2), so this is an initialized value `Box` may take ownership of.
    unsafe { Box::from_raw(ptr) }
}

/// A heap-allocated `T`, built in place and then initialized by `f`.
///
/// The in-place builder for large types. `f` receives an already-valid zeroed `T`, so it can
/// write whatever fields it likes, in any order, and may panic freely: the allocation holds a
/// valid `T` from the first instant, so unwinding drops a well-formed value rather than leaking
/// a partially-written one.
///
/// This is worth stating because the usual `MaybeUninit` version of this pattern *does* have
/// that hazard — unwinding drops a `Box<MaybeUninit<T>>`, which frees the allocation without
/// running any drop glue, silently leaking whatever was already written. For `Pod` types the
/// hazard simply does not arise, because zeroed is always valid and `MaybeUninit` is never
/// needed.
///
/// ```
/// # use portable_pod::{Pod, boxed_zeroed_with};
/// #[derive(Clone, Copy, Pod)]
/// #[repr(C)]
/// struct Big { len: u64, data: [u64; 1024] }
///
/// let b = boxed_zeroed_with::<Big>(|b| b.len = 7);
/// assert_eq!(b.len, 7);
/// ```
#[inline]
#[must_use]
pub fn boxed_zeroed_with<T: Pod>(f: impl FnOnce(&mut T)) -> Box<T> {
    let mut b = boxed_zeroed::<T>();
    f(&mut b);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeroed_box_is_zero() {
        let b = boxed_zeroed::<[u64; 256]>();
        assert!(b.iter().all(|&x| x == 0));
    }

    #[test]
    fn zero_sized_is_allowed() {
        let b = boxed_zeroed::<()>();
        assert_eq!(*b, ());
        let b = boxed_zeroed::<[u32; 0]>();
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn builder_sees_a_valid_value() {
        let b = boxed_zeroed_with::<[u32; 4]>(|v| v[2] = 9);
        assert_eq!(*b, [0, 0, 9, 0]);
    }

    #[test]
    fn large_allocation_does_not_touch_the_stack() {
        // 8 MB: far past a default thread stack. Constructing via `Box::new(zeroed())` would
        // materialize this on the stack first and abort.
        let b = boxed_zeroed::<[u64; 1024 * 1024]>();
        assert_eq!(b[1024 * 1024 - 1], 0);
    }
}
