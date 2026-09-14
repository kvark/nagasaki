//! Turns `src/shaders/*.rs` into WGSL before the crate is compiled.

fn main() {
    synaga::build::Shaders::new()
        // Bindings are assigned by the host at pipeline creation, so the
        // shaders leave `#[group]`/`#[binding]` off.
        .bindings(synaga::build::Bindings::Host)
        .prelude("common.rs")
        .run();
}
