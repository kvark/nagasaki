# nagasaki

Native Rust to [Naga](https://docs.rs/naga) IR transpiler.

Parses a restricted Rust dialect with [`syn`](https://docs.rs/syn) on stable and
builds a `naga::Module` by hand. No `rustc_private`, no nightly.

## v0 dialect

- free functions
- scalars: `f32`, `u32`, `i32`, `bool`
- literals, unary `-/!`, arithmetic / compare / bitwise ops
- `let` / `let x: T = …` (runtime Store + Load; no const init)
- `if` / `else` / `else if` as statement or value
- `x = e` and compound `+=`/`-=`/`*=`/`/=`/… on locals
- implicit tail expressions and `return`

Not yet: `loop`/`while`, references, methods, generics, structs,
entry-point attributes. `!` is `LogicalNot` (bool). Assignment to function
arguments is rejected.

## Example

```rust
use nagasaki::{parse_str, to_wgsl, validate};

let module = parse_str("fn add(a: f32, b: f32) -> f32 { a + b }")?;
let info = validate(&module)?;
println!("{}", to_wgsl(&module, &info)?);
```

## Status

Curiosity / scaffolding. The IR builder is the point — Naga already knows how
to go from there to WGSL, SPIR-V, MSL, and HLSL.
