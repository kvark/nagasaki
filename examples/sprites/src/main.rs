//! The shaders are compiled by `build.rs`; nothing here parses or transpiles.
//!
//! Note there is no `mod shaders;` pointing at `src/shaders/` — those files are
//! not Rust and Cargo never compiles them. The module below is the *generated*
//! one, which is ordinary Rust.

mod shaders {
    include!(concat!(env!("OUT_DIR"), "/shaders.rs"));
}

fn main() {
    for (name, wgsl) in [("sprite", shaders::SPRITE), ("tonemap", shaders::TONEMAP)] {
        println!("--- {name} ({} bytes) ---", wgsl.len());
        println!("{wgsl}");
    }
}
