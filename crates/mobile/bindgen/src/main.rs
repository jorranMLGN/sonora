//! The UniFFI bindings generator, pinned to the version `crates/mobile` is built with. CI runs
//! it over the iOS static library to write the Swift bindings.

fn main() {
    uniffi::uniffi_bindgen_main()
}
