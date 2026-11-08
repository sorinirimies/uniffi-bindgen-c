//! A C bindings generator for [UniFFI](https://mozilla.github.io/uniffi-rs/).
//!
//! Point it at a library built with UniFFI and it writes a C11 header and source
//! file per component, plus one shared runtime header:
//!
//! ```text
//! uniffi-bindgen-c generate target/release/libmycrate.so --out-dir c/
//! ```
//!
//! The generated code calls the scaffolding through its C ABI, so you link the
//! Rust library into your program like any other native library.

mod cli;
mod codegen;
mod config;

pub use cli::run;
pub use codegen::{
    CBindingGenerator, GeneratedFiles, RUNTIME_HEADER, RUNTIME_HEADER_NAME, generate_component,
    generate_from_udl, write_runtime,
};
pub use config::Config;
