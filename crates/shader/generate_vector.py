"""Generates crates/shader/src/vector.rs — the vector types and everything on them."""
from itertools import permutations, product

SCALARS = [("f32", "f32", "float"), ("i32", "i32", "sint"),
           ("u32", "u32", "uint"), ("bool", "bool", "bool")]
SIZES = [2, 3, 4]
# Every vector gets the same operators, differing only in which ones apply:
# `bool` has no arithmetic, only integers shift, only signed types negate. One
# macro says that once; the alternative is 2,500 lines of identical impls.
MACRO = r"""/// The operators every vector has, and the ones only some do.
///
/// `macro_rules!` cannot paste `Add` and `Assign` into one identifier, so each
/// operator names its assigning form too. `$shift` is the `vecNu` a lane-wise
/// shift takes, since WGSL wants an unsigned shift amount whatever is shifted.
macro_rules! vector_ops {
    ($name:ident, $scalar:ty, $shift:ident $(, $group:ident)*) => {
        impl Index<usize> for $name {
            type Output = $scalar;
            #[inline]
            fn index(&self, index: usize) -> &$scalar { unimplemented_on_cpu() }
        }
        impl IndexMut<usize> for $name {
            #[inline]
            fn index_mut(&mut self, index: usize) -> &mut $scalar { unimplemented_on_cpu() }
        }
        $(vector_ops!(@group $group, $name, $scalar, $shift);)*
    };

    (@group arith, $name:ident, $scalar:ty, $shift:ident) => {
        vector_ops!(@scalar_too Add, add, AddAssign, add_assign, $name, $scalar);
        vector_ops!(@scalar_too Sub, sub, SubAssign, sub_assign, $name, $scalar);
        vector_ops!(@scalar_too Mul, mul, MulAssign, mul_assign, $name, $scalar);
        vector_ops!(@scalar_too Div, div, DivAssign, div_assign, $name, $scalar);
        vector_ops!(@scalar_too Rem, rem, RemAssign, rem_assign, $name, $scalar);
    };
    (@group bitwise, $name:ident, $scalar:ty, $shift:ident) => {
        vector_ops!(@lanewise BitAnd, bitand, BitAndAssign, bitand_assign, $name);
        vector_ops!(@lanewise BitOr, bitor, BitOrAssign, bitor_assign, $name);
        vector_ops!(@lanewise BitXor, bitxor, BitXorAssign, bitxor_assign, $name);
    };
    (@group shift, $name:ident, $scalar:ty, $shift:ident) => {
        vector_ops!(@shift Shl, shl, $name, $shift);
        vector_ops!(@shift Shr, shr, $name, $shift);
    };
    (@group neg, $name:ident, $scalar:ty, $shift:ident) => {
        impl Neg for $name {
            type Output = Self;
            #[inline]
            fn neg(self) -> Self { unimplemented_on_cpu() }
        }
    };
    (@group not, $name:ident, $scalar:ty, $shift:ident) => {
        impl Not for $name {
            type Output = Self;
            #[inline]
            fn not(self) -> Self { unimplemented_on_cpu() }
        }
    };

    // Arithmetic also works against a scalar, from either side: a shader
    // writes both `v * 2.0` and `2.0 * v`.
    (@scalar_too $trait:ident, $method:ident, $assign:ident, $assign_fn:ident,
     $name:ident, $scalar:ty) => {
        vector_ops!(@lanewise $trait, $method, $assign, $assign_fn, $name);
        impl $trait<$scalar> for $name {
            type Output = Self;
            #[inline]
            fn $method(self, rhs: $scalar) -> Self { unimplemented_on_cpu() }
        }
        impl $trait<$name> for $scalar {
            type Output = $name;
            #[inline]
            fn $method(self, rhs: $name) -> $name { unimplemented_on_cpu() }
        }
        impl $assign<$scalar> for $name {
            #[inline]
            fn $assign_fn(&mut self, rhs: $scalar) { unimplemented_on_cpu() }
        }
    };
    (@lanewise $trait:ident, $method:ident, $assign:ident, $assign_fn:ident, $name:ident) => {
        impl $trait for $name {
            type Output = Self;
            #[inline]
            fn $method(self, rhs: Self) -> Self { unimplemented_on_cpu() }
        }
        impl $assign for $name {
            #[inline]
            fn $assign_fn(&mut self, rhs: Self) { unimplemented_on_cpu() }
        }
    };
    (@shift $trait:ident, $method:ident, $name:ident, $shift:ident) => {
        impl $trait<$shift> for $name {
            type Output = Self;
            #[inline]
            fn $method(self, rhs: $shift) -> Self { unimplemented_on_cpu() }
        }
        impl $trait<u32> for $name {
            type Output = Self;
            #[inline]
            fn $method(self, rhs: u32) -> Self { unimplemented_on_cpu() }
        }
    };
}"""

XYZW = "xyzw"
RGBA = "rgba"

# Under `rgba`, only the aliases anyone writes: the single components, the
# prefix runs, and the two tails that come up in colour code.
RGBA_ALIASES = [(0,), (1,), (2,), (3,), (0, 1), (1, 0), (2, 3), (0, 1, 2), (0, 1, 2, 3)]


def swizzles(size):
    """Which swizzle methods a vector of `size` lanes gets, as (lanes, letters).

    Every reordering and subset of the lanes it has, and nothing that repeats
    one: `v.xyz()`, `v.zyx()`, `v.yx()`, but not `v.xxyy()`. The exhaustive
    product is 3,984 methods across the twelve types and almost none of them
    are ever called -- it is not free, since every one is compiled by everybody
    who depends on this crate.

    A shader that does want a repeating swizzle can still write `v.xxyy` in the
    field spelling, which the transpiler accepts; `rustc` will not check that
    one, which is the trade.

    Single components are fields under `xyzw`, so those start at two lanes.
    """
    for n in range(2, size + 1):
        for combo in permutations(range(size), n):
            yield combo, XYZW
    for combo in RGBA_ALIASES:
        if max(combo) < size:
            yield combo, RGBA

def vname(size, suffix):
    return f"vec{size}{suffix}"

def suffix_for(scalar):
    return {"f32": "", "i32": "i", "u32": "u", "bool": "b"}[scalar]

out = []
w = out.append

w('''//! Vector types.
//!
//! Generated by `generate_vector.py`, then `cargo fmt`; edit that, not this.
//!
//! Component access splits two ways: a single `x`/`y`/`z`/`w` is a field, so
//! `v.x` reads and `v.x = 1.0` writes, while every other swizzle is a method.
//! Rust has no way to give one piece of memory a hundred overlapping names, so
//! `v.xyz` has to be `v.xyz()`; `.r`/`.g`/`.b`/`.a` are methods for the same
//! reason, since they would alias the `x`/`y`/`z`/`w` fields.
//!
//! Comparisons are methods too. `a < b` in a shader yields one bool per lane,
//! and Rust\'s `PartialOrd` yields a single `bool`, so the lane-wise forms are
//! spelled `cmplt`, `cmple`, and so on, as glam spells them.''')
w("")
w("use core::ops::*;")
w("")
w("use crate::unimplemented_on_cpu;")
w("")
w(MACRO)
w("")

for size in SIZES:
    for scalar, _, kind in SCALARS:
        sfx = suffix_for(scalar)
        name = vname(size, sfx)
        comps = XYZW[:size]
        fields = ", ".join(f"pub {c}: {scalar}" for c in comps)
        args = ", ".join(f"{c}: {scalar}" for c in comps)
        init = ", ".join(comps)
        boolname = vname(size, "b")

        w(f"/// `{name}` in WGSL.")
        w("#[derive(Clone, Copy, Debug, Default, PartialEq)]")
        w("#[repr(C)]")
        w("#[allow(non_camel_case_types)]")
        w(f"pub struct {name} {{ {fields} }}")
        w("")
        w(f"/// Build a [`{name}`] from its components.")
        w("#[allow(non_snake_case)]")
        w("#[inline]")
        w(f"pub const fn {name}({args}) -> {name} {{ {name} {{ {init} }} }}")
        w("")
        w(f"impl {name} {{")
        zero = "false" if scalar == "bool" else "0" if scalar != "f32" else "0.0"
        one = "true" if scalar == "bool" else "1" if scalar != "f32" else "1.0"
        w(f"    pub const ZERO: Self = {name}({', '.join([zero]*size)});")
        w(f"    pub const ONE: Self = {name}({', '.join([one]*size)});")
        w("")
        w("    /// Every lane set to `v`.")
        w("    #[inline]")
        w(f"    pub const fn splat(v: {scalar}) -> Self {{ {name}({', '.join(['v']*size)}) }}")
        if size < 4:
            bigger = vname(size + 1, sfx)
            nc = XYZW[size]
            w("")
            w(f"    /// One lane wider, with `{nc}` appended. This is how a shader\\'s")
            w(f"    /// `vec{size+1}(v, {nc})` is spelled.")
            w("    #[inline]")
            w(f"    pub const fn extend(self, {nc}: {scalar}) -> {bigger} {{")
            w(f"        {bigger}({', '.join('self.' + c for c in comps)}, {nc})")
            w("    }")
        if size > 2:
            smaller = vname(size - 1, sfx)
            w("")
            w("    /// One lane narrower, dropping the last.")
            w("    #[inline]")
            w(f"    pub const fn truncate(self) -> {smaller} {{")
            w(f"        {smaller}({', '.join('self.' + c for c in comps[:-1])})")
            w("    }")
        # lane-wise comparisons
        if scalar != "bool":
            for op, doc in [("cmpeq", "=="), ("cmpne", "!="), ("cmplt", "<"),
                            ("cmple", "<="), ("cmpgt", ">"), ("cmpge", ">=")]:
                w("")
                w(f"    /// Lane-wise `{doc}`.")
                w("    #[inline]")
                w(f"    pub fn {op}(self, rhs: Self) -> {boolname} {{ unimplemented_on_cpu() }}")
        else:
            for op, doc in [("cmpeq", "=="), ("cmpne", "!=")]:
                w("")
                w(f"    /// Lane-wise `{doc}`.")
                w("    #[inline]")
                w(f"    pub fn {op}(self, rhs: Self) -> {boolname} {{ unimplemented_on_cpu() }}")
        # swizzles
        for combo, letters in swizzles(size):
            n = len(combo)
            sw = "".join(letters[i] for i in combo)
            ret = scalar if n == 1 else vname(n, sfx)
            body = (f"self.{XYZW[combo[0]]}" if n == 1
                    else f"{ret}({', '.join('self.' + XYZW[i] for i in combo)})")
            w("")
            w("    #[inline]")
            w(f"    pub const fn {sw}(self) -> {ret} {{ {body} }}")
        w("}")
        w("")

        # Operators are uniform per type, so they go through a macro rather
        # than 2,500 lines of impls that differ only in a name.
        traits = []
        if scalar != "bool":
            traits.append("arith")
        if scalar in ("i32", "u32", "bool"):
            traits.append("bitwise")
        if scalar in ("i32", "u32"):
            traits.append("shift")
        if scalar in ("f32", "i32"):
            traits.append("neg")
        if scalar in ("i32", "u32", "bool"):
            traits.append("not")
        w(f"vector_ops!({name}, {scalar}, {vname(size, 'u')}{''.join(', ' + t for t in traits)});")
        # two-vector concatenation, for a shader's vec4(vec2, vec2)
        if size == 4:
            two = vname(2, sfx)
            w(f"impl From<({two}, {two})> for {name} {{")
            w("    #[inline]")
            w(f"    fn from((a, b): ({two}, {two})) -> Self {{ {name}(a.x, a.y, b.x, b.y) }}")
            w("}")
        w("")

# component-type conversions, for a shader's `v as vec3<f32>`
for size in SIZES:
    for a, _, _ in SCALARS:
        for b, _, _ in SCALARS:
            if a == b:
                continue
            src, dst = vname(size, suffix_for(a)), vname(size, suffix_for(b))
            comps = XYZW[:size]
            w(f"impl From<{src}> for {dst} {{")
            w("    #[inline]")
            w(f"    fn from(v: {src}) -> Self {{ unimplemented_on_cpu() }}")
            w("}")

import pathlib
target = pathlib.Path(__file__).resolve().parent / "src" / "vector.rs"
target.write_text("\n".join(out) + "\n")
print(f"{len(out)} lines -> {target}; now run `cargo fmt`")
