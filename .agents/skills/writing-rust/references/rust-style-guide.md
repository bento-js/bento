# Rust Development Guidelines for Coding Agents

## Purpose

Write idiomatic, correct, maintainable Rust that is easy for humans and automated agents to understand, modify, test, and extend.

Optimize for:

1. **Correctness:** Preserve intended behavior and make invalid states difficult to represent.
2. **Clarity:** Make control flow, ownership, data structures, and invariants understandable.
3. **Idiomatic Rust:** Prefer familiar language features and standard ecosystem conventions.
4. **Simplicity:** Avoid abstractions, dependencies, and indirection without demonstrable value.
5. **Verifiability:** Make behavior testable and correctness checkable through established tooling.
6. **Maintainability:** Favor focused changes, stable interfaces, and easily navigable code.
7. **Performance:** Avoid unnecessary work, but justify optimizations with evidence.

These guidelines describe defaults, not unconditional prohibitions. Requirements imposed by correctness, security, the target environment, or existing project architecture take precedence over stylistic preferences.

## 1. Follow Established Rust Conventions

- Follow the official Rust Style Guide and Rust API Guidelines.
- Use `rustfmt` for formatting. Do not manually enforce competing whitespace conventions.
- Use Clippy to identify common mistakes and non-idiomatic patterns.
- Follow the repository's existing Rust edition, toolchain, supported platforms, and minimum supported Rust version (MSRV).
- Prefer stable language features and standard library APIs unless there is a concrete reason otherwise.
- Use standard Rust naming conventions:
  - `snake_case` for variables, functions, modules, and filenames.
  - `PascalCase` for types and traits.
  - `SCREAMING_SNAKE_CASE` for constants and statics.
- Favor conventional implementations over novel alternatives that provide no meaningful benefit.
- When modifying existing code, preserve established conventions unless changing them is part of the task.

Do not introduce stylistic changes unrelated to the requested behavior.

## 2. Organize Code Around Cohesive Responsibilities

- Give each crate, module, type, and function a clear responsibility.
- Prefer conventional Cargo project layouts.
- Group code by functionality or domain rather than forcing arbitrary file-size limits.
- Split large modules when their responsibilities become difficult to understand.
- Avoid creating many tiny files, modules, or crates merely for organizational purity.
- Prefer shallow, navigable module hierarchies when deeper nesting adds no meaningful distinction.
- Keep implementation details private by default.
- Expose only the smallest public API necessary.
- Use `pub(crate)` for interfaces needed across the current crate but not by downstream consumers.
- Use re-exports deliberately to provide coherent public entry points.
- Avoid multiple publicly accessible paths to the same item unless compatibility requires them.
- Avoid circular architectural dependencies and unnecessary coupling between modules.

Choose module boundaries that help readers understand where a behavior is implemented.

### Imports

- Prefer explicit imports when they make dependencies easier to identify.
- Avoid broad glob imports that obscure the origin of names.
- Accept conventional glob imports where appropriate, including common test imports and established preludes.
- Qualify ambiguous symbols with their module names.
- Avoid introducing local naming conventions that differ unnecessarily from the Rust ecosystem.

## 3. Design APIs for Clarity and Predictability

Make interfaces easy to discover, difficult to misuse, and consistent with standard Rust expectations.

### Inputs and outputs

- Prefer `&str` over `&String` when borrowing string data.
- Prefer `&[T]` over `&Vec<T>` when borrowing a contiguous sequence.
- Prefer `&Path` over `&PathBuf` when borrowing filesystem paths.
- Prefer `&T` over `&Box<T>` when only the underlying value is needed.
- Prefer `Option<&T>` over `&Option<T>` when the optional borrowed value is the intended input.
- Take ownership when the function needs to retain or consume a value.
- Use `impl IntoIterator` when accepting arbitrary iterable inputs provides meaningful flexibility.
- Return iterators when lazy evaluation or composability is useful.
- Return owned collections when the result is naturally materialized or callers need ownership.
- Use `impl Into<T>` or `impl AsRef<T>` when the convenience justifies a more generic signature.

Do not make internal application functions generic solely to avoid simple conversions at their call sites.

### Naming and construction

- Use `new()` for conventional constructors and `try_new()` when construction can fail.
- Use descriptive alternative constructor names such as `from_bytes()` or `with_capacity()` where appropriate.
- Follow Rust's established `as_`, `to_`, and `into_` naming conventions.
- Use `field()` for conventional getters rather than `get_field()`.
- Use `field_mut()`, `set_field()`, or `take_field()` when those operations accurately describe the semantics.
- Prefer named struct fields when positional fields would be difficult to interpret.
- Use ordinary methods for functionality naturally associated with a type.
- Use free functions when no meaningful receiver exists.

### Express semantics through types

- Use enums when parameters represent meaningful choices rather than ambiguous Boolean flags.
- Use newtypes when logically distinct values share the same primitive representation.
- Prefer enums over stringly typed state machines.
- Validate inputs where values cross trust boundaries.
- Make invalid states unrepresentable when the improvement justifies the added type complexity.
- Use builders for complex construction with many optional parameters, but not for simple types that can be initialized directly.
- Implement `Default` only when a sensible default exists.
- Use `#[non_exhaustive]` when preserving the ability to extend a public type is worth limiting downstream exhaustive matching.

Avoid excessive type-level machinery that makes straightforward operations harder to understand.

## 4. Use Rust's Ownership Model Naturally

- Prefer unique ownership and borrowing when they provide a straightforward design.
- Borrow data instead of cloning it unnecessarily.
- Use `.clone()` when an independent value is genuinely required or when it simplifies otherwise disproportionate lifetime complexity.
- Understand whether cloning a value performs a deep copy or merely increments shared ownership.
- Use `Rc` for appropriate single-threaded shared ownership.
- Use `Arc` for appropriate thread-safe shared ownership.
- Use `Mutex`, `RwLock`, atomics, and interior mutability only when their semantics are required.
- Avoid introducing shared mutable state when explicit data flow would be simpler.
- Avoid exposing synchronization primitives in public APIs unless they are part of the intended contract.
- Prefer explicit ownership boundaries over complicated lifetime relationships that yield negligible practical benefits.

Do not indiscriminately add `Arc`, `Mutex`, `Box`, or `.clone()` to resolve compiler errors. Determine the intended ownership relationship first.

Likewise, do not overcomplicate a design to eliminate every allocation or reference count.

## 5. Handle Errors Deliberately

Choose the error mechanism that accurately reflects the operation's contract.

- Use `Option<T>` when absence is an expected outcome and additional failure details are unnecessary.
- Use `Result<T, E>` for operations that can fail in ways callers may need to handle.
- Use `?` for ordinary error propagation.
- Preserve useful error context, particularly around I/O, parsing, external processes, and system boundaries.
- Prefer meaningful error types for reusable library APIs.
- Application-level code may use a general-purpose error type when distinct concrete error variants provide no practical value.
- Avoid discarding errors silently.
- Never convert errors into successful outcomes merely to satisfy the type checker.
- Avoid replacing errors with misleading defaults.

### Panics

- Treat panics as indicators of programming errors or violated invariants, not ordinary failure handling.
- Do not use `unwrap()` or `expect()` on externally controlled data without a justified invariant.
- Prefer pattern matching, `let ... else`, `if let`, or `?` when they express the desired behavior.
- Use `expect()` with a meaningful explanation when a genuine invariant makes failure impossible under correct program behavior.
- Document relevant panic conditions in public APIs.
- Provide useful context in assertions.

A panic should not be used merely because returning an error would require additional code.

## 6. Prefer Standard Traits and Existing Abstractions

- Implement standard library traits when their semantics match the type's behavior.
- Prefer `From`, `TryFrom`, `FromStr`, `AsRef`, `Display`, and other established traits over competing custom conventions.
- Derive trait implementations when automatic behavior is correct.
- Do not derive traits that imply unsupported semantics.
- Implement useful common traits such as `Debug`, `Clone`, `Eq`, `Hash`, or `Default` where appropriate.
- Avoid writing manually implemented versions of traits that can be derived correctly.
- Define custom traits when they represent meaningful shared behavior, polymorphism, or extension points.
- Do not define traits simply to group methods that could be inherent implementations.
- Avoid abstraction layers with only one foreseeable implementation unless they solve a concrete architectural or testing requirement.

### Generics and dynamic dispatch

- Prefer concrete types when flexibility is unnecessary.
- Use generics or `impl Trait` when callers benefit from static polymorphism.
- Use `dyn Trait` when runtime polymorphism, heterogeneous values, or reduced monomorphization are meaningful.
- Avoid exposing highly complex generic signatures without corresponding value.

Choose dispatch based on requirements, not a universal preference for one mechanism.

## 7. Minimize Unnecessary Metaprogramming

Prefer ordinary Rust whenever it expresses the intended behavior clearly.

In general:

1. Prefer functions, structs, enums, traits, and modules.
2. Use declarative macros when they eliminate substantial repetition or enable valuable syntax.
3. Use procedural macros when their capabilities justify additional build and maintenance complexity.
4. Use external code generation when generated artifacts or integration requirements make it appropriate.

Before creating a macro:

- Check whether ordinary functions or traits can accomplish the same task.
- Consider how easily expanded behavior can be inspected and debugged.
- Ensure compiler diagnostics remain useful.
- Avoid surprising name resolution, hidden side effects, and undocumented generated APIs.
- Keep generated interfaces consistent with ordinary Rust conventions.

Do not introduce an entire domain-specific language when conventional APIs are sufficient.

## 8. Write Clear, Direct Implementations

- Prefer straightforward control flow over unnecessarily clever expressions.
- Use `match`, `if let`, and `let ... else` to make conditional behavior explicit.
- Avoid checking an `Option` or `Result` and then separately unwrapping it when pattern matching is clearer.
- Use iterator combinators when they make transformations concise and understandable.
- Prefer ordinary loops when complex iterator chains obscure mutation, branching, or error handling.
- Avoid side effects inside transformations when they make execution difficult to follow.
- Add explicit type annotations where inference would obscure important information.
- Use named constants instead of unexplained magic values.
- Avoid unnecessary intermediate allocations.
- Avoid broad mutable state when local immutable values express the same algorithm.
- Do not replace standard functionality with custom implementations without a specific reason.

Code should make its important behavior apparent without requiring readers to mentally simulate complicated abstractions.

## 9. Use Concurrency and Async Deliberately

- Use synchronous code when synchronous execution satisfies the requirements.
- Use `async` when operations genuinely benefit from asynchronous execution.
- Follow the project's established runtime and concurrency model.
- Avoid blocking operations inside asynchronous execution contexts when they can interfere with runtime scheduling.
- Avoid holding blocking mutex guards across `.await` points.
- Keep synchronization scopes as small as correctness reasonably permits.
- Define clear task ownership, cancellation, shutdown, and error-propagation behavior.
- Do not spawn tasks or threads without understanding their lifetime and resource implications.
- Avoid unnecessary background workers, channels, and synchronization layers.
- Preserve thread-safety guarantees when introducing shared state.
- Handle lock poisoning according to the synchronization primitive's contract and application recovery requirements.

Prefer explicit concurrency relationships over implicit global coordination.

## 10. Restrict and Justify Unsafe Code

- Prefer safe Rust whenever it can meet the requirements.
- Introduce `unsafe` only when necessary for a concrete capability or adequately justified optimization.
- Keep unsafe operations in small, auditable sections.
- Encapsulate unsafe implementation details behind safe interfaces when soundness permits.
- Document the invariants that make each unsafe operation valid.
- Include a `SAFETY:` comment explaining why the required conditions hold.
- Never assume the presence of `unsafe` removes the obligation to preserve soundness.
- Verify relevant pointer validity, initialization, alignment, aliasing, lifetime, and concurrency requirements.
- Use specialized verification tools where available and applicable.

### FFI boundaries

- Keep foreign-function interfaces separate from core domain logic where practical.
- Translate types and errors explicitly at the boundary.
- Document ownership transfer, borrowing duration, allocation responsibilities, and thread affinity.
- Avoid allowing foreign resources to escape into safe Rust interfaces without a sound abstraction.

Unsafe code must be correct, not merely capable of compiling.

## 11. Document Contracts, Not Obvious Syntax

Use Rust documentation conventions consistently:

- `//!` for crate and module documentation.
- `///` for public items.
- `//` for implementation explanations.

Documentation should explain information that cannot be inferred reliably from the signature or implementation.

Prioritize:

- The purpose and intended use of an API.
- Parameter and return-value semantics.
- Ownership or borrowing expectations when non-obvious.
- Error conditions and recovery behavior.
- Panic conditions.
- Safety requirements and invariants.
- Important performance characteristics or side effects.
- Examples for nontrivial public APIs.

Prefer runnable documentation examples that can be checked as doctests.

Do not write comments that merely restate a function name or explain basic Rust syntax.

Avoid historical design narratives in user-facing documentation unless they explain an enduring constraint.

## 12. Make Behavior Independently Testable

- Test externally observable behavior rather than reproducing implementation details.
- Cover normal operation, boundary conditions, malformed inputs, and expected failures.
- Add regression tests for bugs being fixed.
- Prefer deterministic tests with explicit inputs and expected outcomes.
- Avoid tests whose expectations are calculated using the same flawed logic as the implementation.
- Avoid tests that merely assert constants or internal definitions against themselves.
- Use unit tests for focused behavior and integration tests for interactions between components.
- Make external dependencies replaceable or controllable where testing requires it.
- Avoid introducing broad abstraction layers solely for mocking.
- Preserve useful existing tests during refactors.
- Treat failing tests as evidence to investigate rather than obstacles to bypass.

Use the standard Rust test infrastructure unless the repository requires another test framework.

Prefer meaningful behavioral coverage over raw test-count or coverage-percentage targets.

## 13. Optimize Based on Evidence

- Begin with a simple, correct implementation unless performance constraints are already known.
- Be aware of algorithmic complexity, allocations, copying, and avoidable blocking.
- Profile or benchmark before introducing significant micro-optimizations.
- Optimize measured bottlenecks rather than speculative ones.
- Preserve clarity when the performance difference is negligible.
- Document non-obvious optimizations and the assumptions that justify them.
- Avoid introducing unsafe code solely for unmeasured performance gains.
- Verify optimizations against representative workloads.

Performance-sensitive code may justify specialized layouts, explicit allocation management, static dispatch, or other complexity. Such choices should remain understandable and testable.

## 14. Keep Dependencies and Configuration Intentional

- Prefer the standard library when it adequately solves the problem.
- Reuse dependencies already present in the repository when appropriate.
- Add external crates when they provide meaningful capabilities or reduce implementation complexity.
- Evaluate dependency maintenance, compatibility, security, licensing, and target-platform support.
- Avoid dependencies that duplicate existing functionality without clear benefit.
- Do not change the Rust edition, toolchain, or minimum supported version incidentally.
- Respect existing Cargo feature definitions and compatibility requirements.
- Avoid introducing feature combinations that cannot be built correctly.
- Do not introduce global state or build-time behavior without a concrete need.

Prefer reproducible builds and minimize environmental assumptions.

## 15. Agent-Specific Working Rules

### Before modifying code

- Read the relevant crate structure, public interfaces, and nearby implementations.
- Identify existing types, utilities, traits, and patterns that can be reused.
- Determine the requested behavior and the constraints that must remain unchanged.
- Understand the repository's testing, formatting, and linting configuration.
- Check whether a proposed change affects downstream crates or public APIs.

### While implementing

- Prefer the smallest coherent change that fully satisfies the requirements.
- Maintain consistency with surrounding code.
- Do not invent abstractions in anticipation of hypothetical future features.
- Do not duplicate existing functionality.
- Do not translate patterns mechanically from JavaScript, Python, Java, C++, or another language when idiomatic Rust differs.
- Use compiler feedback to identify mistakes, not as justification for weakening invariants.
- Address the underlying causes of borrow-checker and trait errors rather than repeatedly adding clones, boxes, reference counting, or generic bounds.
- Do not suppress warnings without understanding them.
- Keep lint exceptions narrowly scoped and justified.
- Do not modify unrelated code simply because an alternative style is preferred.
- Do not silently change documented behavior, error semantics, or API contracts.

### After implementing

- Review changes for correctness, unintended behavior, and unnecessary complexity.
- Run appropriate formatting, compilation, linting, and testing checks.
- Verify relevant public examples and documentation.
- Check that error handling and edge cases are covered.
- Inspect the final diff for unrelated changes.
- Clearly distinguish verified results from assumptions.
- Never claim a command or test succeeded unless it actually ran successfully.

Compiler success is necessary, but it does not establish behavioral correctness.

## 16. Verification

Use the commands appropriate to the repository.

Typical Cargo checks:

- `cargo fmt --all -- --check` — formatting.
- `cargo check --workspace --all-targets` — compile checking.
- `cargo clippy --workspace --all-targets -- -D warnings` — linting.
- `cargo test --workspace` — tests.
- `cargo doc --workspace --no-deps` — documentation generation.

When appropriate, additionally validate:

- Supported feature combinations and target platforms.
- Public documentation examples and integration tests.
- Minimum-supported-Rust-version compatibility.
- Benchmarks for performance-sensitive changes.
- Unsafe code with tools such as Miri where supported.
- Dependency and security checks where available.

Use `--all-features` only when all features are intended to be enabled simultaneously.

Do not replace a required verification step with another command that checks a weaker property.

## 17. Common Agent Anti-Patterns

Actively avoid:

- Adding unnecessary `.clone()` calls to silence ownership errors.
- Wrapping everything in `Arc<Mutex<T>>`.
- Introducing traits with no meaningful need for polymorphism.
- Building elaborate builder APIs for simple structs.
- Adding abstraction layers merely to make code appear extensible.
- Recreating common standard library functionality.
- Using `unwrap()` on ordinary fallible operations.
- Swallowing errors through `.ok()`, default values, or ignored results without intentional semantics.
- Treating all compiler warnings as problems to suppress.
- Adding broad `#[allow(...)]` attributes to silence Clippy.
- Overusing procedural macros and custom DSLs.
- Translating foreign-language object hierarchies directly into Rust.
- Using unnecessarily complex lifetime or generic machinery.
- Writing tests that restate the implementation instead of validating behavior.
- Adding superficial comments, excessive documentation, or generated process narratives.
- Splitting cohesive implementations into arbitrarily small crates.
- Rewriting functioning code to satisfy a stylistic preference unrelated to the task.
- Optimizing before establishing that an optimization matters.
- Claiming correctness based only on successful compilation.

## Guiding Principle

**Write Rust that makes the correct behavior easy to express, the incorrect behavior difficult to express, and the implemented behavior easy to verify.**

Favor conventional language features, focused abstractions, visible ownership relationships, meaningful errors, and executable tests.

Use Rust's type system and tooling to establish guarantees, without confusing technical sophistication with engineering quality.
