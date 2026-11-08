//! Generate from UDL and check the shape of the output. The compile checks use
//! the system C (and C++) compiler and are skipped when there is none.

use std::process::Command;

use uniffi_bindgen_c::{
    Config, GeneratedFiles, RUNTIME_HEADER, RUNTIME_HEADER_NAME, generate_from_udl,
};

const UDL: &str = r#"
namespace sample {
    u32 add(u32 a, u32 b);
    [Throws=Oops] string risky(string s);
    Shape shape_of(Point p, optional double radius = 2.5);
    [Async] string fetch(string key);
    sequence<string> names();
    record<string, u32> counts();
    bytes reverse(bytes data);
};

dictionary Holder {
    Thing thing;
};

[Error]
enum Oops { "Bad", "Worse" };

[Error]
interface Detailed {
    Code(i32 code);
    Empty();
};

enum Color { "Red", "Green" };

dictionary Point {
    double x;
    double y = 1.5;
    string? label = null;
};

[Enum]
interface Shape {
    Circle(Point center, double radius);
    Empty();
};

interface Thing {
    constructor(string name);
    [Name=other] constructor(u32 n);
    string name();
    [Throws=Oops] void check();
    [Async] u32 later();
};

callback interface Sink {
    void push(string s);
    [Throws=Oops] u32 count();
};
"#;

fn generate() -> GeneratedFiles {
    generate_from_udl(UDL, "sample", &Config::default()).expect("generates")
}

#[test]
fn header_declares_every_kind_of_definition() {
    let h = generate().header;
    for expected in [
        "#ifndef UNIFFI_SAMPLE_H",
        "#include \"uniffi_runtime.h\"",
        "extern \"C\" {",
        "uint32_t sample_add(uint32_t a, uint32_t b, uniffi_status *status);",
        "uniffi_string sample_risky(uniffi_string s, sample_Oops *err, uniffi_status *status);",
        "uniffi_string sample_fetch(uniffi_string key, uniffi_status *status);",
        "void sample_fetch_async(uniffi_string key, void (*done)(void *ctx, uniffi_string result, uniffi_status *status), void *ctx);",
        "sample_SequenceString sample_names(uniffi_status *status);",
        "sample_MapStringU32 sample_counts(uniffi_status *status);",
        "typedef enum sample_Color {",
        "    sample_Color_Red = 0,",
        "struct sample_Point {",
        "#define sample_Point_DEFAULTS {.y = 1.5, .label = {0}}",
        "    sample_Shape_Tag tag;",
        "        } Circle;",
        "typedef struct sample_Thing {\n    /** Owned: release with `sample_Thing_free`. */\n    uint64_t handle;\n} sample_Thing;",
        "sample_Thing sample_Thing_new(uniffi_string name, uniffi_status *status);",
        "sample_Thing sample_Thing_other(uint32_t n, uniffi_status *status);",
        "void sample_Thing_check(sample_Thing self, sample_Oops *err, uniffi_status *status);",
        "void sample_Thing_free(sample_Thing self);",
        "sample_Thing sample_Thing_clone(sample_Thing self);",
        "struct sample_Sink_vtable {",
        "    void (*push)(void *ctx, uniffi_string s, uniffi_status *status);",
        "    uint32_t (*count)(void *ctx, sample_Oops *err, uniffi_status *status);",
        "void sample_Point_free(sample_Point *value);",
        "void sample_SequenceString_free(sample_SequenceString *value);",
    ] {
        assert!(h.contains(expected), "header lacks `{expected}`:\n{h}");
    }
}

#[test]
fn ffi_declarations_stay_out_of_the_header() {
    let files = generate();
    assert!(!files.header.contains("uniffi_sample_fn_func_add"));
    assert!(files.source.contains(
        "extern uint32_t uniffi_sample_fn_func_add(uint32_t a, uint32_t b, RustCallStatus *out_status);"
    ));
}

#[test]
fn source_calls_the_scaffolding_and_checks_status() {
    let s = generate().source;
    for expected in [
        "#include \"sample.h\"",
        "sample_uniffi_ensure_init();",
        "RustCallStatus uniffi_call = {0};",
        "uniffi_check_call(&uniffi_call, status, sample_uniffi_lift_error_Oops, err)",
        "uniffi_future_block_on(uniffi_future, (uniffi_poll_fn)ffi_sample_rust_future_poll_rust_buffer);",
        "static void sample_Sink_uniffi_init(void) {",
    ] {
        assert!(s.contains(expected), "source lacks `{expected}`");
    }
}

#[test]
fn config_prefix_overrides_the_udl_namespace() {
    let config = Config {
        prefix: Some("smp".into()),
        ..Config::default()
    };
    let files = generate_from_udl(UDL, "sample", &config).expect("generates");
    assert!(
        files
            .header
            .contains("uint32_t smp_add(uint32_t a, uint32_t b, uniffi_status *status);")
    );
    assert!(files.header.contains("#ifndef UNIFFI_SMP_H"));
    // The file names follow the UniFFI namespace.
    assert_eq!(files.header_name, "sample.h");
}

#[test]
fn config_omit_checksums_drops_the_check() {
    let with = generate().source;
    let config = Config {
        omit_checksums: true,
        ..Config::default()
    };
    let without = generate_from_udl(UDL, "sample", &config)
        .expect("generates")
        .source;
    assert!(with.contains("API checksum mismatch"));
    assert!(!without.contains("API checksum mismatch"));
    assert!(without.contains("contract version mismatch"));
}

fn compiler(candidates: &[&'static str]) -> Option<&'static str> {
    candidates.iter().copied().find(|c| {
        Command::new(c)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// Compiles the generated source as C11 (and the header as C++17 when a C++
/// compiler exists). Without a C compiler the check passes vacuously.
fn compiles(files: &GeneratedFiles) -> bool {
    let Some(cc) = compiler(&["cc", "clang", "gcc"]) else {
        eprintln!("no C compiler; skipping the compile check");
        return true;
    };
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join(RUNTIME_HEADER_NAME), RUNTIME_HEADER).expect("write");
    std::fs::write(dir.path().join(&files.header_name), &files.header).expect("write");
    std::fs::write(dir.path().join(&files.source_name), &files.source).expect("write");
    let out = Command::new(cc)
        .args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Wpedantic",
            "-Werror",
            "-c",
        ])
        .arg(dir.path().join(&files.source_name))
        .arg("-I")
        .arg(dir.path())
        .arg("-o")
        .arg(dir.path().join("out.o"))
        .output()
        .expect("run the C compiler");
    if !out.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&out.stderr));
        return false;
    }
    let Some(cxx) = compiler(&["c++", "clang++", "g++"]) else {
        return true;
    };
    let out = Command::new(cxx)
        .args([
            "-x",
            "c++",
            "-std=c++17",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fsyntax-only",
        ])
        .arg(dir.path().join(&files.header_name))
        .arg("-I")
        .arg(dir.path())
        .output()
        .expect("run the C++ compiler");
    if !out.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&out.stderr));
    }
    out.status.success()
}

#[test]
fn generated_code_compiles() {
    assert!(compiles(&generate()), "generated code does not compile");
}

#[test]
fn empty_namespace_generates_compilable_code() {
    let files =
        generate_from_udl("namespace empty {};", "empty", &Config::default()).expect("generates");
    assert!(compiles(&files));
}

#[test]
fn invalid_udl_is_an_error() {
    assert!(generate_from_udl("namespace broken { void f(", "broken", &Config::default()).is_err());
}

// uniffi 0.32 passes `[ByRef] bytes` (`&[u8]`) as `ForeignBytes`: lent, not copied.
#[test]
fn borrowed_bytes_are_lent_without_a_copy() {
    let udl = "namespace lend { u32 checksum([ByRef] bytes data); };";
    let files = generate_from_udl(udl, "lend", &Config::default()).expect("generate");
    assert!(files.source.contains("uniffi_borrow_bytes(data)"));
    assert!(
        files
            .header
            .contains("uint32_t lend_checksum(uniffi_bytes data, uniffi_status *status);")
    );
    assert!(compiles(&files));
}

// Regression: a flat error's message ("Variant: text") was built by copying past
// the end of the variant name.
#[test]
fn flat_error_message_joins_variant_and_text_in_a_fresh_buffer() {
    let s = generate().source;
    assert!(s.contains("text.data = (char *)uniffi_alloc(total + 1);"));
    assert!(!s.contains("uniffi_string_copy(variant, n + 2"));
}

// Regression: a container of custom types needs the custom typedef before it,
// even though it only holds pointers.
#[test]
fn container_of_custom_type_comes_after_the_typedef() {
    let udl = "namespace cust { sequence<Name> names(); }; [Custom] typedef string Name;";
    let files = generate_from_udl(udl, "cust", &Config::default()).expect("generate");
    let h = &files.header;
    let typedef = h.find("typedef uniffi_string cust_Name;").expect("typedef");
    let container = h.find("struct cust_SequenceName {").expect("container");
    assert!(typedef < container, "{h}");
    assert!(compiles(&files));
}

#[test]
fn generated_header_snapshot() {
    insta::assert_snapshot!("sample_header", generate().header);
}

#[test]
fn generated_source_snapshot() {
    insta::assert_snapshot!("sample_source", generate().source);
}
