# Formal methods in tree-police: a learning path

A self-study curriculum for Lean and for fuzzing in CI, where every exercise
lands on real code in this repo. The pieces are already wired up; each phase
explains one of them and gives you something to change.

## What's here

```text
src/suppress.rs        parse_directive: pure parser for `tree-police: ignore…` comments
formal/                Lean 4 project (core Lean only, no Mathlib)
  TreePolice/Severity.lean   warm-up model + proofs for src/severity.rs
  TreePolice/Suppress.lean   model of parse_directive and Suppressions::apply + proofs
  Oracle.lean                the model as a CLI: JSON string in, JSON parse out
tests/props.rs         proptest properties (run by plain `cargo test`)
tests/lean_oracle.rs   differential test: Rust vs the Lean model, same random inputs
fuzz/                  cargo-fuzz targets (libFuzzer, coverage-guided)
.github/workflows/verify.yml   all of the above on every PR; longer fuzzing nightly
```

The three techniques answer different questions:

| Technique | Question it answers | Strength | Blind spot |
| --- | --- | --- | --- |
| Property tests (`tests/props.rs`) | Does the invariant hold on thousands of generated inputs? | Cheap, shrinks failures to a minimal input | Only as good as the generator |
| Fuzzing (`fuzz/`) | Can *any* input crash it or break an assertion? | Evolves inputs using coverage, finds odd paths | Needs an assertion to check; no notion of "correct" |
| Lean proofs (`formal/`) | Does the property hold for *every* input? | Exhaustive, machine-checked | Proves things about the *model*, not the Rust |
| Differential test (`tests/lean_oracle.rs`) | Does the Rust behave like the model? | Ties the proofs back to the shipped code | Sampling again: agreement on tested inputs only |

The last row is the important one. A proof about a model you never compare
with the real code only tells you about the model. AWS's Cedar policy
language works this way: a Lean specification with proofs, plus differential
random testing of the Rust implementation against it. tree-police does the
same thing on a much smaller scale.

## Commands

```bash
direnv allow             # dev shell now includes elan and cargo-fuzz
just test                # unit + CLI + query cases + property tests
just lean                # build formal/, which checks every proof
just lean-oracle 200000  # Rust vs Lean on 200k generated comments
just fuzz suppress_python 300
```

The first `just lean` downloads the Lean toolchain pinned in
`formal/lean-toolchain` (about 600 MB).

## Phase 0: orientation (an evening)

- Watch the talk at the top of your notes and read the Anthropic piece on
  formal verification. The point to take away: an LLM may write a proof, but
  the Lean kernel checks it. If `lake build` passes and there is no `sorry`,
  you can trust the proof without trusting whoever (or whatever) wrote it.
  That's why the combination is interesting.
- Skim `src/suppress.rs` and `formal/TreePolice/Suppress.lean` side by side.
  Match each Rust line to its Lean line.

## Phase 1: Lean basics (1 to 2 weeks)

**Resources:** [Natural Number Game](https://adam.math.hhu.de/) (Tutorial,
Addition and Multiplication worlds), then chapters 1 to 5 of
[Theorem Proving in Lean 4](https://leanprover.github.io/theorem_proving_in_lean4/)
(dependent types, propositions, quantifiers, tactics).

**What to learn:** `theorem`, `by`, `rfl`, `simp`, `cases`, `induction`,
`intro`, `exact`, and how to read the goal view in VS Code with the Lean 4
extension.

**Exercises in this repo:**

1. In `Severity.lean`, prove `ofStr? "bogus" = none`, then a general version:
   `ofStr? s = some x → s ∈ ["log", "info", ...]`. Hint: `unfold ofStr?` and
   `split`.
2. `Severity` has no `LE` instance. Define `a ≤ b := a.rank ≤ b.rank` and
   prove transitivity and totality. That's the order `--min-severity` relies
   on.
3. In `Suppress.lean`, prove `splitComma_ne_nil : splitComma s ≠ []` by
   induction on `s`. The comment calls the `[] => [[]]` branch "unreachable";
   this proof shows it is.

## Phase 2: Lean as a programming language (1 week)

**Resources:** [Functional Programming in Lean](https://lean-lang.org/functional_programming_in_lean/),
chapters 1 to 4 (structures, type classes, monads, `IO`).

**What to learn:** how `Oracle.lean` reads stdin, parses JSON, and calls the
model. Why the model works on `List Char` rather than `String`: proofs about
lists are plain structural induction, while the `String` API is built for
fast execution and is harder to reason about.

**Exercises:**

1. Model `Selector::parse` (`src/config.rs`) in Lean. Add a second oracle
   mode, e.g. a `{"selector": "..."}` input line, and a second differential
   test.
2. Prove the round-trip property for your model:
   `parse (display s) = some s` whenever `parse` accepted the input. The
   Rust version is in `tests/props.rs` and `fuzz/fuzz_targets/selector_roundtrip.rs`.
   You now have the same claim checked three ways.

## Phase 3: differential testing (a weekend)

**What to learn:** a model that's wrong in a way you didn't anticipate is the
usual failure, and differential testing is how you catch it.

**Exercises:**

1. In `Suppress.lean`, change `trimStart` to use `Char.isWhitespace` instead
   of `isWs`, then run `just lean-oracle`. proptest finds the mismatch and
   shrinks it to `"tree-police:\u{a0}0"`: Lean's `Char.isWhitespace` is
   ASCII-only, Rust's `char::is_whitespace` is Unicode. Revert the change.
2. Make the Rust side wrong instead, e.g. `split_word` without `-`. The same
   test fails from the other direction.
3. Remove the Unicode-whitespace arm from the generator in
   `tests/lean_oracle.rs` and redo exercise 1. How many cases does it take to
   find the bug now? This is the "only as good as the generator" blind spot
   in practice.

## Phase 4: fuzzing in CI (a weekend)

**Resources:** the [Rust Fuzz Book](https://rust-fuzz.github.io/book/)
(cargo-fuzz chapters), and libFuzzer's
[options](https://llvm.org/docs/LibFuzzer.html#options) (`-max_total_time`,
`-dict`, `-only_ascii`).

**What to learn:** a fuzz target is a property test where the fuzzer, not
you, decides what to try next, steered by code coverage. The corpus is the
fuzzer's memory between runs. `verify.yml` caches it so every CI run starts
from where the previous one stopped.

**Exercises:**

1. `just fuzz suppress_python 120`, then `cargo fuzz coverage suppress_python`
   to see which lines it reached.
2. Write a `fuzz/dict/directive.dict` containing `"tree-police:"`,
   `"ignore-next-line"`, `"["`, `"category:"`. Pass it with `-dict=` and see
   how much faster new coverage appears.
3. Write a target for config-file parsing (`src/config.rs`). Decide what it
   should assert, beyond "doesn't panic".
4. Read the `fuzz` job in `verify.yml`: 60 s per target on PRs, 15 min
   nightly, crash inputs uploaded as artifacts. Change the nightly budget and
   trigger it with `workflow_dispatch`.

## Phase 5: write the spec first (ongoing)

So far the model follows the code. Now let it lead.

Building the model turned up this behaviour (pinned in both test suites):

```python
print(x)  # tree-police: ignore [forbidden_print]   <- note the space
```

Because of the space, `[forbidden_print]` is not read as a selector list. The
comment becomes a bare `ignore` and suppresses **every** rule on that line.
`src/suppress.rs` already promises that "an unknown selector never widens to
'every rule'", and this is the same kind of silent widening.

**Exercise:**

1. Decide the intended behaviour: accept the space, or warn and drop the
   directive.
2. Write it as a Lean theorem **before** changing any code, e.g. "if the text
   after the directive word is whitespace followed by `[`, the result is
   never `selectors := none`".
3. Change the Lean model until the theorem is provable, then change the Rust.
   `just lean-oracle` tells you when the two agree again. Update the pinned
   tests (`space_before_selector_list_widens_to_every_rule` in
   `tests/props.rs`, and the `example` in `Suppress.lean`).

This is spec-first development in miniature, and a good task to hand to an
LLM: the theorem defines "done", the kernel checks the proof, and the
differential test checks the Rust.

## Phase 6: going further

- [Mathematics in Lean](https://leanprover-community.github.io/mathematics_in_lean/)
  is the textbook for Mathlib. Use it once you need real algebra or order
  theory; this repo deliberately avoids Mathlib to keep builds fast.
- **Kani** (bounded model checking of Rust): it checks the Rust itself for
  all inputs up to a size bound. Try proving `split_word` never panics.
- **Aeneas** translates safe Rust into Lean, so you prove things about the
  real code rather than a hand-written model.
- **Verus** is Rust with proof annotations, checked by SMT.
- [Lean Zulip](https://leanprover-community.github.io/meet.html), `#new
  members` stream: the place to ask when a proof won't go through.
