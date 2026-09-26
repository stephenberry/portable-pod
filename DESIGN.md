# portable-pod — design

The README is the user-facing argument. This is the implementer's doc: what the contract is, how each clause is discharged, what the mechanism cannot do, and what is left to build.

## 1. The contract

A type may implement `Pod` only if all four hold.

| # | Clause | Discharged by |
| --- | --- | --- |
| 1 | `Copy + 'static` | supertrait bound |
| 2 | Any bit pattern is valid | induction: every field is `Pod`, bottoming out at a hand-audited axiom set |
| 3 | No padding | mechanically, at compile time (§2) |
| 4 | Position-independent | induction, same as clause 2 |

Clause 4 is the one that separates this from `bytemuck::Pod`, which covers `usize`. Violating it does not produce undefined behavior on one machine; it produces a value that disagrees with itself across machines, which is worse, because no single-target test can see it.

The axiom set is the trusted base: the integer scalars, `()`, `[T; N]`, and `Bit`. It is small, fixed, and does not grow when a user adds a type. Everything above it is proved.

## 2. How padding is proved

One assertion, and its completeness is the point:

```rust
assert!(size_of::<Self>() == size_of::<F0>() + size_of::<F1>() + size_of::<F2>());
```

This single equation proves the absence of padding **everywhere**, internal and tail alike. `repr(C)` places each field at the next offset that is a multiple of its alignment and rounds the total up to the struct's own alignment; every one of those roundings can only *increase* the size. So the total equals the sum exactly when no rounding did anything, which is exactly when there is no padding. Internal padding cannot hide behind a compensating shortfall elsewhere, because there is no shortfall to be had.

The strictness is load-bearing, and `#[repr(C)] struct { a: u32, b: u64 }` is why. It has four bytes of internal padding at `4..8`, and its size (16) equals the field sum (12) *rounded up to the alignment* (8) — so a size check that tolerated that rounding would pass it. The equation above does not, and `tests/ui/internal_padding_aligned.rs` pins that. This is also why the relaxed variant discussed in §4 was unsound in a second way beyond the one that got it removed.

**Every field is proved `Pod` on its own**, never merged with another field whose type is spelled the same: identical tokens can name different types (§12 has the case, and the release it broke). For a generic type that proof is a where-clause bound per field; for a concrete type it is the impl's body naming `<Field as Pod>`, and the impl is unconditional (§3).

**The proof must also be transitive.** A field type's proof is only evaluated when its const is *named*, and `size_of::<Inner<1>>()` names `size_of`, not `<Inner<1> as Pod>::__LAYOUT_OK`. So the derive additionally emits `let _: () = <FieldTy as Pod>::__LAYOUT_OK;` for every field type, and the blanket array impl forwards its element's proof rather than taking the empty default.

Without both of those, a padded generic was accepted whenever it was merely *contained* in another type instead of reaching an entry point itself: the container's own checks passed vacuously (one field, offset 0, sizes equal), and `[Inner<1>; 2]` erased the proof entirely. Found by adversarial review after the first implementation, confirmed under Miri as a real read of uninitialized memory, and now covered by `tests/ui/contained_generic_padding.rs` and `tests/ui/array_of_padded_generic.rs`. It is the easiest mistake to make in this design, because the `Pod` *bound* on a field looks like it discharges the obligation and does not: it proves `Inner<1>: Pod`, not that `Inner<1>`'s layout was ever checked.

Earlier versions also emitted a per-field `offset_of!` assertion, so the error could name the field the gap precedes. Those were removed: they are redundant for soundness, and measurement showed they *were* the derive's cost at the use site — around 85% of it, at every struct size. §5.1 has the numbers and the reasoning. The diagnostic they bought is now paid for statically instead, in the assertion message.

## 3. When the check fires

This is the mechanism's real limitation and it needs stating plainly.

| Type | Proof | Caught by |
| --- | --- | --- |
| concrete | `const _: () = <T as Pod>::__LAYOUT_OK;` at the definition, forced unconditionally | `cargo check` |
| generic | associated const, evaluated per monomorphization | `cargo build` / `cargo test` |

A generic type has no single layout to check: `Ring<3>` and `Ring<4>` are different structs. So the proof is an associated const, which rustc evaluates once per instantiation, and every safe entry point in the crate opens with `prove_layout::<T>()` to force it.

The consequence is that a padded generic instantiation is a **post-monomorphization error**. Type-checking does not monomorphize, so `cargo check` and `cargo clippy` will not see it. CI must run `cargo build` or `cargo test`, and the workflow says so where it would otherwise be tempting to economize.

This used to be untestable as well as awkward. `trybuild` type-checks its fixtures, so no compile-fail fixture could demonstrate the failure, and the suite documented the hole rather than covering it. `nocompile` drives `cargo build`, so `tests/ui/padded_generic_instantiation.rs` now pins the whole mechanism: the golden names `<Ring<3> as Pod>::__LAYOUT_OK` specifically, records the chain through `prove_layout::<Ring<3>>` from `bytes_of`, and passes only because `Ring<4>` in the same fixture is accepted. That one file asserts per-instantiation proof, entry-point forcing, and post-monomorphization timing at once.

The scope needs stating exactly, because it is narrower than it first looks: an instantiation is checked when it **reaches this crate** — passed to an entry point, or a field of a type that is (§2's transitivity rule). A generic type the program only constructs and reads directly is checked by nothing. `assert_layout::<T>()` exists so a user can close that gap deliberately. It is a `const fn`, so `const _: () = assert_layout::<Ring<3>>();` beside the type proves the instantiation wherever the item is compiled, `cargo check` included, and `tests/ui/assert_layout_const_item.rs` pins that; called from a function, it is proved when that function is monomorphized.

Within that scope it is still strictly more than a hand-written `assert_eq!(size_of::<Ring<8>>(), 36)` provides, which covers exactly one instantiation.

A second consequence, same root cause: for a *generic* type, an unsatisfiable field bound (`where &'a u32: Pod`) makes the impl inapplicable rather than being an error at the definition, so that diagnostic also lands at the use site. `tests/ui/field_reference.rs` is written around this.

A concrete type has no field bounds at all. Its impl is unconditional, and each field is proved `Pod` by the impl's own body, which names `<Field as Pod>` and is type-checked at the definition whether or not anything uses the type. That is exactly as sound: a field that is not `Pod` fails the build, at the field. What changed in derive 0.2.1 is how often. Through 0.2.0 a concrete impl carried `where usize: Pod`, which is an error at the definition and *also* leaves the impl unusable, so every use of the type failed its own `Pod` bound: `bytes_of`, `read_pod`, `zeroed`, `assert_layout`, `SHAPE`, a containing struct, an array of it, a generic container's instantiation. `tests/ui/concrete_field_not_pod_used.rs` does all of those: 15 errors under 0.2.0, one now. A padded concrete type is likewise one error, from the proof forced at its definition, however it is used (`tests/ui/concrete_padding_used.rs`).

Each non-`Pod` field is named `Pod` in two bodies, `__LAYOUT_OK`'s inherited proof and `SHAPE`'s projection, and with no bound to discharge them both report it. The two diagnostics are identical, on the field's span. A non-incremental build (`--release`, the compile-fail harness) deduplicates them in rustc; an incremental one, whose spans record the body they came from, emits both and cargo prints one, so a reader sees one error and a count of two. A single rustc diagnostic was possible and was not taken. Moving the inherited proofs into `SHAPE`'s body, beside the projections, reports the field once, but then `__LAYOUT_OK` has to force `SHAPE` to remain the whole transitive proof, and a `SHAPE` evaluated at every definition breaks a program that compiled before: one whose `shape_with` panics when evaluated and that never reads the shape (review found it; `lazy_shape` in `tests/derive.rs` pins it). Leaving the proofs in `SHAPE` without forcing it reports once and keeps the shape lazy, but then a concrete type's `__LAYOUT_OK` is no longer transitive, and a padded field is caught only because rustc also evaluates the constants that an unevaluated associated const's body names, which is observed behaviour rather than a guarantee this crate should rest soundness on. So the expansion is 0.2.0's minus the field bounds: `SHAPE` is lazy, `__LAYOUT_OK` is the whole proof, and the duplicate is cargo's to fold.

This changes no accepted program. For a type with no generic parameters, a bound `F: Pod` holds exactly when the body's `<F as Pod>` is well-formed, so the same set of types derives, with an equivalent impl, the same `__LAYOUT_OK` and the same lazily evaluated `SHAPE` (`golden_derive_forms` in `tests/shape.rs` and `golden_shapes` in `tests/cross_crate.rs` pinned representative forms before the change). Only where and how often a rejected program is reported moved: the `field_*` goldens lost the error each used to carry at the struct's name, a field type of several tokens is now underlined whole, `cross_crate_non_pod_field.stderr` lost the errors at the macro call and at its `read_pod`, and `field_not_copy.stderr` lost one of its `Copy` errors.

## 4. Why there is no escape hatch for tail padding

An earlier version shipped `#[pod(tail_padding_is_zero)]`, for a fixed-capacity container whose trailing alignment bytes are always zero because every instance originates from zeroed construction and nothing writes past the last field. It relaxed only the tail assertion; the per-field offset checks still ran, so internal padding stayed proved absent.

**It was removed because it could not be used soundly, for a reason no assertion could reach.** Returning a value by value is a *typed copy*, and a typed copy leaves padding bytes uninitialized regardless of what was written there before. So `zeroed()`, `read_pod()`, and every struct literal produced a value whose padding was uninitialized, and `bytes_of` on it was undefined behavior. The obligation the documentation asked the user to accept — "every instance originates from zeroed construction" — was not satisfiable with the API the crate provides. `boxed_zeroed` was the only sound constructor, because the value never leaves its heap allocation and so is never typed-copied, and it was not the one the docs pointed at.

Found by adversarial review and confirmed under Miri, which reported the crate's *own* test for the feature as UB. That test had passed CI, because the Miri job ran `--lib` and the test lived in an integration target.

Two lessons worth keeping, because someone will propose re-adding it:

- **A padding-tolerant `Pod` cannot coexist with a by-value `Copy` API.** Supporting it would mean denying `zeroed`, `read_pod`, `bytes_of(&T)` and `Copy` itself for such types, which one trait cannot express.
- **The derive cannot supply the fix itself.** A derive may not add a field to the item it is given, so it cannot insert the padding. The user writes `_pad`, and the error message says so.

## 5. Why a proc macro, and why not `syn`

`macro_rules!` was considered and rejected on three counts. It cannot be a `#[derive]`, so every POD struct would have to live inside a macro body, degrading goto-definition, field completion, rustdoc, and IDE rename. It cannot inspect `#[repr]`, because `$(#[$meta:meta])*` captures attributes opaquely — and that check is load-bearing, since `repr(Rust)` does not necessarily *add* padding (it reorders to minimize it) but does make layout unstable across compilations, which no const or runtime check can detect. And generics would need a tt-muncher to split declaration form from use form.

`syn` was rejected because this derive never inspects a field type. A type is captured as an opaque token run and re-emitted into `size_of::<...>()` and a where-clause bound, nothing more. The parsing surface is attributes, visibility, an ident, generic parameters, angle-bracket nesting, and a field list. `src/parse.rs` is that front end; if a future feature needs real type inspection, revisit.

Adversarial review of the parser found four defects, and their distribution is the useful part: **every one was in the generic-parameter and where-clause surface, and none was in field-type handling.** The justification above held exactly where it claimed to. The defects were a missing arrow guard in the default-cut loop (a `->` inside a bound underflowed the angle depth and truncated the parameter), a `where` scanner that stopped at a braced const argument believing it was the struct body, a spacing test that missed a default written `= *const u32`, and attributes on generic parameters being rejected. All four are fixed and locked down by `parser_regressions` in `tests/derive.rs` — and all four are things `syn` would have handled for free, which is the honest cost of this decision.

The where-clause bounds, which only a generic type gets (§3), use field *types*, never generic parameters. The same clause covers `[T; N]` and `u32` alike, so the derive never has to reason about type parameters or ask the user to write a bound.

### 5.1 What the derive costs its users, and where that cost lives

Build-time dependencies are no longer a number at all: there are none. The derive was written against `proc-macro2` and `quote` at first, and both were removed. That took **0.89s off a cold build down to 0.33s**, once per target directory — but the compile time was the smaller half of the reason. The larger half is that this crate's claim is "your bytes depend on nothing", `derive` is a default feature, and a guarantee that lapses the moment someone accepts the default is not much of a guarantee. It is now unconditional, and §5.2 records what the port cost.

The cost that scales is at the **use site**, and it is mostly not the macro. Two independent measurements say so. First, hand-writing the exact impl the derive emits costs +2.19s where the derive costs +2.87s over 200 structs of 48 fields, so expansion is about a quarter of the total and rustc's handling of the emitted code is the rest — **which a `macro_rules!` version would pay identically.** Macro technology is not the lever; the volume of generated code is.

Second, that volume has been reduced twice, and this is the history because both steps are easy to undo by accident.

**The quadratic.** Stating field *k*'s expected offset as the sum of the sizes of fields `0..k` emits `N*(N+1)/2` `size_of` terms per struct. Expressing it relative to the predecessor emits `2N` and telescopes to the same statement. At 48 fields: +2.87s → +1.05s.

**The per-field checks themselves.** Those were then removed outright in favour of the single size equation of §2, which is a complete proof on its own. Measured over 200 derived structs (`cargo check`, min of 3, delta against the same structs without `Pod`):

| fields per struct | with per-field checks | size equation only |
| --- | --- | --- |
| 4 | +0.11s | +0.03s |
| 8 | +0.20s | +0.03s |
| 16 | +0.39s | +0.06s |
| 48 | +1.11s | +0.08s |

The shipped form is **essentially flat in field count**; the per-field checks were roughly 85% of the derive's total cost at every size, not just at the extreme. (`Pod::SHAPE` has since added a term that is linear in field count, by choice and measured; §12 has the numbers.) Two probes locate that cost at 48 fields: N `assert!`s with no layout expressions at all cost +0.26s, because const panic sites are not free, and N `offset_of!` calls cost a further +0.23s.

Guarding the per-field asserts behind the cheap check recovers nothing — measured +1.11s, no better than leaving them unguarded — because the cost is type-checking and MIR construction, which happen whether or not const-evaluation reaches them. There is no arrangement that keeps the detail for free.

**What was given up, and what replaced it.** The per-field asserts let the error name the field the gap precedes. The size equation cannot: it reports only that the struct has padding. That detail is now paid for statically instead, in the assertion message, which lists the fields in declaration order with their types and states the `repr(C)` placement rule — the information needed to locate the gap by hand, and information that is *not* otherwise available when the struct was itself generated by another macro and does not appear in the source. Note that the span was never the carrier here: rustc points the error at `#[derive(…)]`, not at a field, so the message text has always been doing all of the work.

The trade is deliberate and the direction was chosen knowing the numbers above: for a human-sized struct, locating the gap is a minute of `repr(C)` arithmetic, and the compile time is paid on every build by everyone.

### 5.2 Dropping `proc-macro2` and `quote`

`proc_macro` is a sysroot crate, so a derive can be written with no third-party code at all. The two crates the first implementation used were there for reasons that do not apply here.

**`proc-macro2`** exists mainly so expansion logic can run *outside* a proc-macro invocation, where `proc_macro`'s types panic. That buys unit tests of `expand`. This crate has none and wants none: it is tested through the compiled macro by `tests/derive.rs` and the compile-fail fixtures, which exercise the real thing rather than a stand-in, and a compile-fail fixture is a strictly better test of a derive than an assertion about a token stream.

**`quote`** is quasi-quoting sugar. `proc_macro::TokenStream` implements `FromStr`, so a `lex("…")` helper over ordinary format strings replaces it, and for output this formulaic the result reads about the same.

The port was small, and its shape is worth recording because it predicts the cost of doing this to another derive:

- `parse.rs`, 459 of the derive's 628 lines, needed **nine one-line changes**. `TokenTree`, `Group`, `Punct`, `Literal`, `Delimiter` and `Span` are API-identical between the two crates. The sole difference is that `proc_macro::Ident` does not implement `PartialEq`, so `id == "struct"` does not compile; `parse::is` is that comparison.
- `lib.rs` needed real work: 14 `quote!` sites became stream concatenation.

**The one thing worth being careful about is spans**, and it is the reason not to reach for `format!(…).parse()` wholesale. Tokens lexed from a string all carry `Span::call_site()`, which for a derive points at the `#[derive(Pod)]` attribute. The `usize: Pod` diagnostic is only useful because it points at the *field* instead. `quote_spanned!` did that; here `respan` does, and user tokens are concatenated in verbatim so they keep the spans they arrived with. `tests/ui/field_usize.stderr` pins the result at `8:13`, the column of the offending type.

That the whole compile-fail suite passed **byte-for-byte** across this change is the evidence that the port preserved behaviour, spans included. It is worth keeping that property: a change to the emitter that alters no fixture has almost certainly altered nothing.

## 6. Endianness is a boundary, not a caveat

The guarantee is about layout: size, offsets, absence of padding, absence of target-dependent widths. `bytes_of` yields the native representation, so integer byte order is the target's.

Every mainstream *configuration* is little-endian, and WebAssembly is little-endian by specification. The architectures are not: ARM and RISC-V are bi-endian (Rust ships `aarch64_be-unknown-linux-gnu`), and `s390x` and `powerpc64` are big-endian tier-2 targets. So the gap is real, and it used to be handled by documenting it in three places. It no longer is.

**The crate refuses to compile for a big-endian target**, and the reasoning is §1's, applied to itself. Clause 4 excludes `usize` because a value that disagrees with itself across machines is worse than one that is unsound on a single machine, *no single-target test being able to see the disagreement* being the whole point. Byte order is that failure exactly. Leaving it documented rather than enforced meant `s390x` compiled clean, proved every layout, and handed back bytes no little-endian peer could read.

The old arrangement was worse than merely silent — it was fail-open. The digest assertion carried `#[cfg(target_endian = "little")]`, so the one test that could have observed a byte-order disagreement removed itself on exactly the target that would have one. That `cfg` is gone and the assertion is unconditional, which it can now be.

This converts endianness from a hole in the guarantee into a **boundary** of it: inside the supported targets the promise is unconditional, which is a thing a name and a README can state without a footnote. `allow-big-endian` opts out for a caller who wants the layout proofs alone. CI does **not** pin this: asserting that a build fails requires a big-endian target in the matrix, and the `s390x` job was cut as an obscure platform. The guard is therefore reasoned, not tested, and a `cfg` typo that stopped it firing would be invisible. If you touch that `cfg`, check it by hand: `cargo check -p portable-pod --target s390x-unknown-linux-gnu` must fail on the `compile_error!`, and `--features allow-big-endian` must then succeed.

Overclaiming here would be easy and wrong. `usize` is excluded not because byte order is unfixable but because a `usize` has no fixed *size*, and no amount of byte-swapping recovers from that.

## 7. Testing

The compile-fail suite is the primary deliverable, not a supplement: a derive that accepts an unsound type is worse than a hand-written `unsafe impl`, because it launders a bad assertion through machinery that looks authoritative. 47 fixtures in `tests/ui/`, covering each clause, with `field_usize.rs` as the one that distinguishes this crate from `bytemuck`.

Beyond it: `tests/derive.rs` for the positive direction, `tests/portability.rs` for the cross-width property, `tests/shape.rs` for the frozen shape values (§12), `tests/cross_crate.rs` with the unpublished `cross-crate-fixture` crate for types that only another crate can spell (§12), Miri over the unsafe blocks, and a `cargo tree` assertion that the runtime crate has no dependencies at all.

## 8. Not done

- **Benchmarks.** Nothing here should be slower than a transmute, but "should" is not "measured".
- **Big-endian is refused, but the refusal is no longer tested.** `cargo check --target s390x-unknown-linux-gnu` fails on the `compile_error!` when run by hand; with the `s390x` job cut from CI, nothing enforces that it keeps doing so. The opt-out is weaker still: `--features allow-big-endian` was only ever compiled for `s390x`, never executed, so what `bytes_of` yields there was already reasoned rather than demonstrated and is now unbuilt as well.
- **`read_pod_slice` and a slice-reading counterpart.** Deliberately absent until someone needs one.
- **The derive is not fuzzed.** The parser is hand-rolled, and the four defects in §5 were found by hand-written adversarial cases rather than systematically. A fuzzer over generated struct definitions is the next real increment in confidence.

Discharged since the first draft: Miri runs clean over `--lib`, `--test derive` and `--test portability` (the latter two were not covered before, and `--test derive` is where the UB was); the README's examples compile as doctests via `#[cfg(doctest)]`; and the feature matrix runs tests rather than only builds.

## 9. Being re-exported

The derive expands to paths, and a path has to be rooted somewhere. Rooting it at `::portable_pod` is correct for a direct dependant and wrong for everyone downstream of a crate that re-exports the trait — which is the normal way a library hands its users one vocabulary. Those users depend on the library, not on this crate, so `::portable_pod` does not resolve for them and the derive is unusable no matter how cleanly the trait itself re-exports.

This was found by adopting the crate into a library that re-exports `Pod` from its own `mem` module: the trait, `Bit` and the byte accessors all re-exported without incident, and then almost every site that wanted the derive — the ones whose hand-written padding arithmetic the derive exists to replace — could not have it.

`#[pod(crate = <path>)]` roots the expansion elsewhere. Three decisions in it:

- **A path, not a string.** `serde` and `bytemuck` both take a string, for a syn-shaped reason this crate does not have: with no `syn`, the attribute's tokens are already a token stream, and re-lexing a string literal would *discard* the spans they arrived with. Taking the path directly means a typo inside it is reported at the typo. A string literal is therefore an error, and the error names the unquoted replacement, because arriving from either of those crates and writing quotes is the likely mistake.
- **Only the trait has to be reachable.** The expansion names `<path>::Pod` and nothing else, so a re-exporting crate needs one `pub use portable_pod::Pod;`. Requiring more would make the attribute a coupling to this crate's internals.
- **No inference.** There is no attempt to detect the re-export automatically. `$crate` is available to `macro_rules!` and not to a derive, and guessing from the call site would be a heuristic that fails silently in exactly the case it was added for.

The regression test is `tests/ui/pod_crate_wrong_path.rs`, which points the attribute at a module that does not export `Pod`. It pins the **impl header** — reverting that one site to a hardcoded `::portable_pod` changes the golden. It does *not* pin the others (a generic type's field bound, the transitive `__LAYOUT_OK`, the concrete forced proof, and the two in `SHAPE`): the field bound, the transitive proof and `SHAPE`'s per-field projection are respanned onto the same field span and mask each other, and the forced proof and `SHAPE`'s `<() as Pod>::__SHAPE_FOLD` dedupe against the header's error. No trybuild fixture can pin those, because `::portable_pod` resolves inside any fixture crate; catching them needs a consumer that does not depend on this crate, which is where the bug was found in the first place.

## 10. `Pod: Copy` for a generic type

`Pod` requires `Copy`, so `unsafe impl Pod for T` obliges the compiler to prove `T: Copy`. For a generic struct with a derived `Copy` — `impl<K: Copy, V: Copy> Copy for Table<K, V>` — that means proving `K: Copy, V: Copy`, and the derive's field-type bounds do not supply it. `[K; CAP]: Pod` does not let the solver conclude `K: Copy`: that would mean reasoning backwards through the blanket `impl<T: Pod, const N: usize> Pod for [T; N]`, which is not something trait solving does.

So the derive emits `T: ::core::marker::Copy` for each **type** parameter. `Copy` rather than `Pod`: it is the supertrait obligation exactly, and bounding the parameters `Pod` would additionally reject a type whose fields are `Pod` without every parameter being so — reachable through a hand-written impl on an inner type — which is a judgement about type parameters this derive has no business making (§5's rule that it never inspects a field type has the same root). Lifetimes and const parameters are excluded: a lifetime cannot be `Copy`, and emitting `'a: Copy` is a *syntax* error that would fail the whole item.

The bug survived first release because both generic fixtures in `tests/derive.rs` happened to declare the bound inline (`Queue<T: Copy, const N: usize>`, `Guarded<T> where T: Copy`), and the derive copies a struct's own parameter list verbatim into the impl. Putting bounds on the impls instead of the struct is the more common style and was entirely unrepresented. `Unbounded` and `Mixed` now cover both, and `Mixed` is deliberately never constructed — for it, compiling *is* the assertion, since either wrong parameter kind fails the file rather than a test body.

## 11. Size and alignment pins

The padding proof answers "are there gaps?", not "is this the layout the bytes were written against?". A type can change size and stay padding-free, and for a crate whose reason to exist is bytes that agree across machines, a silent size change is the same failure moved in time: this build disagrees with last month's save file instead of with another target. Users were writing a runtime `assert_eq!(size_of::<T>(), N)` test beside nearly every such type to catch it. That test is a compile-time fact checked at run time, in a test binary, on whichever targets happen to run tests. `#[pod(size = N, align = A)]` moves it to the definition, where every build of every target checks it.

Three decisions in it:

- **A type error, not an assertion, for a concrete type.** The check is `let _: [(); N] = [(); size_of::<T>()];`. Array lengths are compared during type checking, so `cargo check` reports it, and rustc renders both lengths: "expected an array with a size of 12, found one with a size of 16". The second number is what someone updating a pin needs, and an `assert!` in a const cannot print it, because const panics cannot format integers. The generated tokens are respanned onto the pinned value, so the error lands inside the user's attribute. The wording says "size" even for an alignment pin; pointing at `align = 16` makes that readable, and a clearer message would cost the found value.
- **An assertion for a generic type.** An array length may not depend on a generic parameter, so the pin is an `assert!` inside `__LAYOUT_OK` and is checked per instantiation, exactly as the padding proof is (§3). rustc names the failing instantiation, so the message says what was pinned; it cannot say what was found.
- **Values are expressions, captured as tokens and re-emitted.** That keeps the no-`syn` rule of §5, lets a pin name a constant, and lets a generic pin be an identity over its parameters (`size = 4 * N + 4`) rather than one number. The cost is the one ambiguity token-level splitting has: `<` is indistinguishable from a generic-argument opener, so `size = 1 << 4, align = 8` would swallow the `align`. A value whose angle brackets do not balance is refused with the parenthesised spelling (`tests/ui/pod_size_bare_shift.rs`); a `>` or `>>` cannot unbalance anything and is accepted.

Alignment is the one pinnable property that is not portable by construction: a padding-free type's size is the sum of its fields everywhere, but a `u64`'s alignment is 8 on 64-bit targets and 4 on 32-bit x86. An `align = 8` pin failing on i686 is therefore correct, not a false positive, and the answer is `repr(C, align(8))`. The docs say so rather than weakening the pin.

## 12. Shape hash

A size pin (§11) catches a layout that grew or shrank. It cannot catch one that changed without changing size: two `u32` fields swapped, a field renamed, a `u32` retyped as an `i32`. The bytes still fit and read back as the wrong values. `Pod::SHAPE: Option<u64>` is a hash of a type's field structure that a consumer writes into a file, replay or wire header and compares on load, refusing an image written against another shape instead of misreading it.

**It is a persistence format.** Consumers persist the value, so the algorithm is public (`portable_pod::shape` documents it exactly: constants, encoding, built-in tags), and it changes only in a semver-major release. `tests/shape.rs` pins exact values for every built-in scalar, an array, a nested struct, a tuple struct, a unit struct, a generic struct at two const arguments, `shape_with`, and the `None` case; those goldens are the freeze. The same file re-implements the algorithm from the docs alone, sharing no code with the crate, and checks it reproduces the goldens, so the documentation is verified to be the algorithm. CI runs the file on `wasm32-wasip1` as well, because a shape must be the same on every target.

What is in and out, and why:

| In | Out |
| --- | --- |
| each field's name and shape, in declaration order (names NFC-normalized as rustc holds them; tuple fields by index, `"0"`, `"1"`, …; `r#type` as `type`) | the type's own name |
| each const generic parameter's value, in declaration order | `repr`, alignment, `align(N)` |
| `#[pod(shape_with = <u64 expr>)]`, if given | field visibility, attributes, generic parameter names |
| `size_of::<Self>()` | a const generic parameter's type |

- **Type names are out** so that renaming a type does not invalidate every save that stored one. The cost is that `Tick(u64)` and `Money(u64)` share a shape, so a field retyped from one to the other is not detected. `shape_with` is the escape for a type that must be told apart, and its intended use is a wrapper storing an enum's discriminant as an integer, which folds in a hash of the variant table so that reordering the enum refuses old data.
- **Alignment is out** because it varies by target (a `u64` is 4-aligned on i686) and a shape must not. It is also redundant: the padding proof makes the size the sum of the field sizes, and the fields' shapes already determine those.
- **Const parameters are in** because they can carry meaning without changing layout: `Fixed<32>` and `Fixed<24>` have identical fields. Each is cast `as u128`, lossless for every type a stable const parameter can have (integers, `bool`, `char`), and absorbed as two words. The parameter's type is not folded, so values that widen alike collide across types: `true` as a `bool` and `1` as a `u8`, or `-1` as an `i8` and `u128::MAX`. Folding the type would mean emitting its tokens as a string, and a parameter retyped without changing meaning is not a change worth refusing data over. Type parameters need nothing: they reach the shape through the fields that use them.
- **Wrapping is a change.** `struct Tick(u64)` is a struct with one field named `0`, not a `u64`, so a field retyped from `u64` to `Tick` refuses old data. That is conservative, and the conservative direction is the right one for a check whose failure mode is misreading. A newtype that should be its field for this purpose says so with `#[pod(transparent)]` (§13).
- **`None` is contagious.** A hand-written impl reports `None` unless it states a shape, which is what makes adding the item a minor release: every existing impl compiles and honestly reports that it has no shape. Any struct or array containing a `None` is `None`, so a `Some` always covers the whole type. A hand-written impl can compute a real one with `shape::scalar`, `shape::array` and `shape::Fold`, the same functions the derive uses.

**The algorithm.** A 64-bit state absorbs 64-bit words as `s = mix(s ^ w)`, where `mix` is the SplitMix64 finalizer (a bijection with full avalanche), starting from the first 64 fractional bits of π. Every record opens with a kind word (scalar, array, struct, field, parameter, extension, end) and has a length fixed by what precedes it (a name is its byte length, then its bytes packed eight to a little-endian word), so the encoding is unambiguous and two different structures agree only by a 64-bit collision. It is not cryptographic: a collision can be constructed on purpose, and nothing here defends against that, since the threat is a programmer's edit, not an adversary. The odds that matter are per comparison: a reader compares one type's stored shape with its current one, and an edited type goes undetected with probability 2^-64. Across a program's few thousand types, the chance that *any* two share a shape by accident is around 2^-40, which matters only to someone using shapes as type identifiers, which they are not (type names are out).

**Reached through the trait.** §9 promises a re-exporting crate that `Pod` is the only item it has to re-export. A derive that called `<root>::shape::Fold::new()` would break that promise, and every existing `#[pod(crate = ...)]` user, in a minor release. So the trait carries a hidden `const __SHAPE_FOLD: shape::Fold`, the expansion reads it as `<() as <root>::Pod>::__SHAPE_FOLD`, and calls `Fold`'s `const fn` methods on the value: method resolution does not need the type's path to be nameable. `()` because only this crate can implement `Pod` for it, so no impl can override the starting point.

**The emitted const**, and what it cost. It is one expression, lazily evaluated (rustc evaluates an associated const only where something names it), with no panic site and no forced layout proof: a const panic site costs at every derive (§5.1), and forcing `__LAYOUT_OK` would be redundant beside the entry points that already force it while adding a second "erroneous constant" trail to every padding diagnostic. Nothing forces `SHAPE` either, for a concrete type as for a generic one, so a `shape_with` that cannot be evaluated fails only a program that reads the shape (§3 has the release where that nearly changed). Every path in it is absolute (`::core::primitive::u64`, `::core::option::Option`), so a `type u64 = u32;` in the user's scope changes nothing; `shadowed_names` in `tests/shape.rs` pins that, and the size pins' `usize` got the same treatment.

**Fields are never merged by spelling.** Through 0.1.4 the derive emitted one bound and one inherited proof per *distinct* field type, comparing types by their token text, and the first cut of `SHAPE` shared projections the same way. That was unsound. `$crate::Header` prints the same whichever crate's macro wrote it, so a struct built by one crate's macro, with a field of its own `$crate::Header` and a field of the caller's `$crate::Header`, had its second field never bounded: a non-`Pod` field in a `Pod` struct, and an invalid `bool` from `read_pod` under Miri. `tests/cross_crate.rs` and `tests/ui/cross_crate_non_pod_field.rs`, with the `cross-crate-fixture` crate as the other crate, are the regression tests. No merge rule based on tokens can be proved safe. Rejecting any type containing `$crate` would cover stable Rust, but a `macro` (macros 2.0, nightly) resolves a plain `Header` at its definition site, so two identical token runs with no marker at all can name different types. So every field gets its own bound, its own inherited proof and its own projection.

The same investigation found a second hygiene bug in the same place. The derive moved the `#[pod(crate = ...)]` path onto each field's span to point diagnostics at the field, and `$crate` resolves through its span, so on a field written by another crate's macro the path named that crate. A span is a location and a hygiene context, and only the location should move. A user-given path now has its tokens relocated with `Span::located_at`, which moves the location to the field and keeps the context it was written in, so `$crate` still names the crate whose macro wrote it and an unsatisfied bound is still reported at the field (`tests/ui/pod_crate_field_not_pod.rs`). The default `::portable_pod`, the derive's own token, is still moved outright, which keeps every other field diagnostic exactly where it was (§5.2); relocating it instead worsened about a dozen of them. A typo in a `crate = ...` path is reported once per field, as it already was.

Measured with the §5.1 harness: 200 derived structs of `u32` fields, `cargo check`, delta against the same structs without `Pod`, minimum of 20 interleaved rounds, rustc 1.98, Apple M1 Max. Every field of every struct has the same type, which is the worst case for giving up the merge:

| fields per struct | 0.1.4 (merged, unsound) | `SHAPE`, merged (not shipped) | shipped: per field |
| --- | --- | --- | --- |
| 4 | +0.04s | +0.06s | +0.07s |
| 16 | +0.07s | +0.10s | +0.19s |
| 48 | +0.15s | +0.22s | +0.52s |

A second run under heavier machine load gave +0.18s, +0.32s and +0.65s at 48 fields: the absolute numbers move, and the ratios hold. Reading `SHAPE` for every struct adds nothing measurable. The cost is type-checking the emitted code at every derive, whether or not anything reads it. Each of the three per-field emissions (the where-clause bound, the inherited proof, the `SHAPE` projection) accounts for about a third of the difference between the merged and per-field columns. A struct whose fields have mostly distinct types, which is the usual case, pays close to the merged figure, because the merge had nothing to save there. (The first column differs from §5.1's table because the toolchain and machine do; the comparison is within one run.)

Two choices keep the shipped column where it is. A `.field(name, <F as Pod>::SHAPE)` method call per field cost measurably more than passing every field in one `.fields(&[(name, shape), …])` call. Packing the names into one NUL-separated string beside an array of shapes would save a little more, about 3µs a field, and was declined: it needs a hidden, stringly protocol between the derive and the runtime.

One sound saving was left here for a separate decision, and derive 0.2.1 took it for concrete types. A field type that mentions none of the struct's generic parameters needs no where-clause bound, because the body's `<F as Pod>` already requires `F: Pod` and fails at the definition when it is not. For a type with no generic parameters that is every field, so a concrete impl has no field bounds; that drops one of the three per-field emissions, about a third of the per-field cost above, and it is what makes a wrong field one error rather than one per use (§3). A generic type keeps a bound on every field, including those that mention no parameter: for it, dropping them would turn §3's use-site error into a definition-site one for some fields and not others, which is a larger change than this one needed.

The compile-fail goldens: dropping the merge changed one, `pod_crate_wrong_path.stderr`, which now reports the second `u64` field's unresolvable path too. That second field was exactly the one the merge used to skip. `field_pointer.stderr` quotes the scalar impls' macro invocation, which now carries each scalar's tag. `pod_unknown_key.stderr` lists the new argument.

**Derive and runtime versions.** The derive now emits items the runtime must have (`SHAPE`, `__SHAPE_FOLD`), so a runtime must never be paired with any derive but its own. The runtime therefore depends on the derive with an exact `=` version, as `serde` does on `serde_derive`, and keeps doing so. The published 0.1.4 runtime asks for `^0.1.4`, which would admit a derive 0.1.5 that emits `SHAPE`. A project holding the runtime at 0.1.4 would then fail to compile after `cargo update -p portable-pod-derive` or a bot's update. So the derive is versioned on its own and the one that emits `SHAPE` is 0.2.0, which no 0.1.x runtime accepts, paired with runtime 0.1.5. From here each runtime release names its derive exactly. A release commit changes the two version lines (the workspace version and the derive's) and the `=` requirement, nothing else. Publish with `cargo publish --workspace`, or `cargo publish -p portable-pod-derive -p portable-pod`, which orders the derive first and skips the unpublished `cross-crate-fixture`; `cargo package --workspace` trips over that fixture instead. The README's "Versions" section states the rule for users.

## 13. Transparent newtypes

`#[pod(transparent)]` gives a one-field struct its field's shape: `Tick(u64)` with it has `u64`'s shape, where without it `Tick` is a struct with one field named `0` (§12). Nothing else changes. The layout proof is the ordinary one, so the field's proof is inherited and the size equation still runs, and a `repr(C, align(8))` newtype over a `u32` fails it (`tests/ui/transparent_padding.rs`, whose message says to drop the `align`, since the usual advice to add a padding field cannot apply). It takes `repr(transparent)` or `repr(C)`, which lay out one field identically. A concrete newtype's impl is unconditional and a generic one's bounds its field, as for any type (§3).

**Is forwarding the right identity?** For a fingerprint the question is which edits must refuse old bytes, and forwarding answers that wrapping never does. That is right for a newtype that is a compile-time distinction over bytes meaning the same thing either way: a typed ID or index, a unit, a handle a macro generates for another type. A save that stored the raw `u32` and one that stored `UserId(u32)` are the same data, a reader should accept either, and retyping a field from one to the other should not invalidate every file. It is wrong for a newtype whose wrapping changes meaning. With both forwarding, a field retyped from `u64` to `Tick`, or from `Tick` to `Money`, reads back without complaint. The second was already true without the attribute, since type names are out (§12); the first is what the attribute gives up, and it is the user's statement that nothing is lost by it.

**Why it is opt-in.** Every newtype derived so far has the struct shape, and those shapes are persisted: `golden_derive_forms` in `tests/shape.rs` pins `repr(transparent)` and single-field `repr(C)` newtypes, tuple and named, in it. Forwarding by default, or inferring it from `repr(transparent)`, would change all of them, which §12 allows only in a semver-major release. `repr` also stays out of shapes on principle (§12's table), so switching a newtype between `repr(C)` and `repr(transparent)` changes no shape. The flip side is that adding the attribute to an existing type, or removing it, changes that type's shape and every shape containing it, like any field edit; `a_transparent_shape_is_its_fields` shows both sides.

**What it refuses rather than drops.** The attribute says the newtype and its field are one thing to anything reading a shape, so anything the default fold would add on top is an error: a second field, even a zero-sized one whose name the fold would record (`tests/ui/transparent_two_fields.rs`); a `shape_with` (`transparent_shape_with.rs`); and any const parameter (`transparent_const_param.rs`). The default derive folds a const parameter's value in, so `Fixed<32>` and `Fixed<24>` have different shapes; forwarding keeps them apart only if the field's shape depends on the parameter. That cannot be decided from tokens (§5): a field of type `Ignore<N>`, where `type Ignore<const N: usize> = u32;`, mentions `N` and discards it, and a braced argument `{ const N: usize = 4; N }` mentions a different `N`. A first cut accepted any parameter whose identifier appeared in the field's type, and review bypassed it both ways. So every const parameter is refused, including one the field plainly uses, such as `Lanes<const N: usize>([u16; N])`, whose shape would have been `[u16; N]`'s. Allowing that case later, once something can prove the dependence, is a compatible change; refusing a type that is accepted today would not be. A type parameter needs no such rule, because the default derive does not fold type parameters either, only the fields that use them (§12), so forwarding loses nothing the default would have kept.

## 14. Non-goals

- Competing with `bytemuck`. If your bytes never leave the machine, use it.
- Floats. Excluded by clause 2, because NaN payloads are not stable across targets.
- A general casting library: no alignment-changing casts, no `cast_slice`.
- Serialization. This is a layout guarantee, not a format. It composes with one.
