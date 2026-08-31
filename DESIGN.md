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

The scope needs stating exactly, because it is narrower than it first looks: an instantiation is checked when it **reaches this crate** — passed to an entry point, or a field of a type that is (§2's transitivity rule). A generic type the program only constructs and reads directly is checked by nothing. `assert_layout::<T>()` exists so a user can close that gap deliberately in a test.

Within that scope it is still strictly more than a hand-written `assert_eq!(size_of::<Ring<8>>(), 36)` provides, which covers exactly one instantiation.

A second consequence, same root cause: for a *generic* type, an unsatisfiable field bound (`where &'a u32: Pod`) makes the impl inapplicable rather than being an error at the definition, so that diagnostic also lands at the use site. For a concrete type the bound is checked immediately. `tests/ui/field_reference.rs` is written around this.

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

The where-clause bounds use field *types*, never generic parameters. An unsatisfied concrete bound is a hard error, and the same clause covers generic field types, so the derive never has to reason about type parameters or ask the user to write a bound.

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

The shipped form is **essentially flat in field count**; the per-field checks were roughly 85% of the derive's total cost at every size, not just at the extreme. Two probes locate that cost at 48 fields: N `assert!`s with no layout expressions at all cost +0.26s, because const panic sites are not free, and N `offset_of!` calls cost a further +0.23s.

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

The compile-fail suite is the primary deliverable, not a supplement: a derive that accepts an unsound type is worse than a hand-written `unsafe impl`, because it launders a bad assertion through machinery that looks authoritative. 25 fixtures in `tests/ui/`, covering each clause, with `field_usize.rs` as the one that distinguishes this crate from `bytemuck`.

Beyond it: `tests/derive.rs` for the positive direction, `tests/portability.rs` for the cross-width property, Miri over the unsafe blocks, and a `cargo tree` assertion that the runtime crate has no dependencies at all.

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

The regression test is `tests/ui/pod_crate_wrong_path.rs`, which points the attribute at a module that does not export `Pod`. It pins the **impl header** — reverting that one site to a hardcoded `::portable_pod` changes the golden. It does *not* pin the other three (the field bound, the transitive `__LAYOUT_OK`, and the concrete forced proof): the first two are respanned onto the same field span and mask each other, and the third dedupes against the header's error. No trybuild fixture can pin those, because `::portable_pod` resolves inside any fixture crate; catching them needs a consumer that does not depend on this crate, which is where the bug was found in the first place.

## 10. `Pod: Copy` for a generic type

`Pod` requires `Copy`, so `unsafe impl Pod for T` obliges the compiler to prove `T: Copy`. For a generic struct with a derived `Copy` — `impl<K: Copy, V: Copy> Copy for Table<K, V>` — that means proving `K: Copy, V: Copy`, and the derive's field-type bounds do not supply it. `[K; CAP]: Pod` does not let the solver conclude `K: Copy`: that would mean reasoning backwards through the blanket `impl<T: Pod, const N: usize> Pod for [T; N]`, which is not something trait solving does.

So the derive emits `T: ::core::marker::Copy` for each **type** parameter. `Copy` rather than `Pod`: it is the supertrait obligation exactly, and bounding the parameters `Pod` would additionally reject a type whose fields are `Pod` without every parameter being so — reachable through a hand-written impl on an inner type — which is a judgement about type parameters this derive has no business making (§5's rule that it never inspects a field type has the same root). Lifetimes and const parameters are excluded: a lifetime cannot be `Copy`, and emitting `'a: Copy` is a *syntax* error that would fail the whole item.

The bug survived first release because both generic fixtures in `tests/derive.rs` happened to declare the bound inline (`Queue<T: Copy, const N: usize>`, `Guarded<T> where T: Copy`), and the derive copies a struct's own parameter list verbatim into the impl. Putting bounds on the impls instead of the struct is the more common style and was entirely unrepresented. `Unbounded` and `Mixed` now cover both, and `Mixed` is deliberately never constructed — for it, compiling *is* the assertion, since either wrong parameter kind fails the file rather than a test body.

## 11. Non-goals

- Competing with `bytemuck`. If your bytes never leave the machine, use it.
- Floats. Excluded by clause 2, because NaN payloads are not stable across targets.
- A general casting library: no alignment-changing casts, no `cast_slice`.
- Serialization. This is a layout guarantee, not a format. It composes with one.
