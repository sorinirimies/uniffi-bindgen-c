#![no_main]
//! Whatever text we are given, parsing it as UDL either fails cleanly or yields
//! a definition the generator renders without panicking; a rendered header
//! always has its include guard and C++ linkage block.

use libfuzzer_sys::fuzz_target;
use uniffi_bindgen_c::{Config, generate_from_udl};

fuzz_target!(|data: &[u8]| {
    let Ok(udl) = std::str::from_utf8(data) else { return };
    if let Ok(files) = generate_from_udl(udl, "fuzz", &Config::default()) {
        assert!(files.header.contains("#ifndef UNIFFI_FUZZ_H"));
        assert!(files.header.contains("extern \"C\" {"));
        assert!(files.source.contains("#include \"fuzz.h\""));
    }
});
