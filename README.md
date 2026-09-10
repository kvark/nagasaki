# nagasaki

Native Rust to [Naga](https://docs.rs/naga) IR transpiler.

Parses a restricted Rust dialect with [`syn`](https://docs.rs/syn) on stable and
builds a `naga::Module` by hand. No `rustc_private`, no nightly.

## v0 dialect

- free functions
- scalars: `f32`, `u32`, `i32`, `bool`
- vectors: `vec2`/`vec3`/`vec4` (default `f32`), `vecN<T>`, `vec2f`/`vec3i`/`vec4u`
- constructors: `vec3(x, y, z)`, splat `vec3(x)`, mix `vec3(xy, z)`
- components: `.x`/`.y`/`.z`/`.w`, swizzle `.xy`/`.zyx`, index `v[0]` / `v[i]`
- literals, unary `-/!`, arithmetic / compare / bitwise ops (scalar splat on mix)
- `let` / `let x: T = …` (runtime Store + Load; no const init)
- `if` / `else` / `else if` as statement or value
- `x = e` and compound `+=`/`-=`/`*=`/`/=`/… on locals
- `loop` / `while` / `break` / `continue`
- implicit tail expressions and `return`
- entry points: `#[vertex]` / `#[fragment]` / `#[compute]` + `#[workgroup_size(x,y,z)]`
- bindings: `#[location(N)]`, `#[builtin(name)]` on args; `#[output(builtin(..))]` / `#[output(location(N))]` on the fn

Not yet: labeled loops, `break` values, `for`, component stores (`v.x =`),
references, methods (`dot`/`cross`), matrices, generics, structs, `@group`/`@binding`
globals. `!` is `LogicalNot` (bool). Assignment to function arguments is
rejected. Vector compare yields a `vecN<bool>`. Interpolation is filled in
(`perspective` for floats, `flat` for integers) on vertex outputs / fragment inputs.

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
