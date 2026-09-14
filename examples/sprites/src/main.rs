//! The shaders are compiled twice: `rustc` checks `src/shaders/` as ordinary
//! Rust, and `build.rs` reads the same files and transpiles them to WGSL.

mod shaders;

mod wgsl {
    include!(concat!(env!("OUT_DIR"), "/shaders.rs"));
}

fn main() {
    for (name, source) in wgsl::ALL {
        println!("--- {name} ({} bytes) ---", source.len());
        println!("{source}");
    }
}
