# nagasaki

Native Rust to [Naga](https://docs.rs/naga) IR transpiler.

Parses a restricted Rust dialect with [`syn`](https://docs.rs/syn) on stable and
builds a `naga::Module` by hand. No `rustc_private`, no nightly.

## v0 dialect

- free functions
- scalars: `f32`, `u32`, `i32`, `bool`
- vectors: `vec2`/`vec3`/`vec4` (default `f32`), `vecN<T>`, `vec2f`/`vec3i`/`vec4u`
- matrices: `mat2`/`mat3`/`mat4` (square `f32`), `matCxR`, `mat4f`, `mat2x3<f32>`
- constructors: `vec3(x, y, z)`, splat `vec3(x)`, mix `vec3(xy, z)`
- matrix constructors: column vectors `mat4(c0,c1,c2,c3)` or column-major scalars
- components: `.x`/`.y`/`.z`/`.w`, swizzle `.xy`/`.zyx`, index `v[0]` / `v[i]` / `m[0]`
- literals, unary `-`/`!`, arithmetic / compare / bitwise / shift ops (scalar splat on mix)
- casts: `a as f32`, `v as vec3<u32>` (same width, component-wise)
- `let` / `let x: T = …` (runtime Store + Load; no const init)
- `if` / `else` / `else if` as statement or value
- `x = e` and compound `+=`/`-=`/`*=`/`/=`/… on locals
- `loop` / `while` / `break` / `continue`
- implicit tail expressions and `return`
- entry points: `#[vertex]` / `#[fragment]` / `#[compute]` + `#[workgroup_size(x,y,z)]`
- bindings: `#[location(N)]`, `#[builtin(name)]` on args; `#[output(builtin(..))]` / `#[output(location(N))]` on the fn
- calls to earlier free functions
- math builtins: `dot`, `cross`, `normalize`, `length`, `distance`, `abs`, `min`, `max`, `clamp`, `mix`, `sin`, `cos`, `transpose`, `determinant`, …
- globals: `#[group(N)] #[binding(M)] static x: T = ();` (init ignored) or `extern { static x: T; }`
- address spaces: uniform (default / `#[uniform]`), `#[storage]` (read), `#[storage(read_write)]`
- structs: `struct S { a: vec3, b: f32 }`, literals `S { a, b: x }`, field access `s.a`
- I/O structs: `#[location]` / `#[builtin]` on struct fields, for vertex outputs,
  fragment inputs, and multiple render targets

Not yet: labeled loops, `break` values, `for`, component stores (`v.x =` / `s.a =`),
forward calls, methods, generics, arrays, textures and samplers, `void` functions.
Assignment to function arguments is rejected. Vector compare yields a `vecN<bool>`.

### Typing

Operand rules follow Naga's, so a program the frontend accepts is a module its
validator accepts — `-x` needs a signed or float operand, `a & b` an integer or
bool one, a shift amount is always `u32`, and so on.

Untyped integer literals take their type from context the way Rust's inference
would: `n << 1`, `n * 2`, `clamp(n, 0, 10)`, `f(1)`, `let n: u32 = 1` and
`vec3u(1, 2, 3)` all work whatever integer type is in play. There is no
`1` to `1.0` conversion, again as in Rust.

A function with a return type has to return on every path; `if c { a }` as a
whole body is rejected rather than quietly falling off the end.

A function you declare shadows a math builtin of the same name.

### Interpolation

On entry-point arguments and results, interpolation is filled in where WGSL
needs it — `perspective` for floats, `flat` for integers — on vertex outputs and
fragment inputs.

Struct fields are shared between stages, so only the float default is applied
there. An integer `#[location]` field that is interpolated has to say so:

```rust
struct VsOut {
    #[builtin(position)] pos: vec4,
    #[location(0)] uv: vec2,
    #[location(1)] #[flat] material: u32,
}
```

A struct is either plain data or a shader interface: binding some fields and not
others is rejected. An entry point returning a bound struct does not take
`#[output(...)]`.

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
