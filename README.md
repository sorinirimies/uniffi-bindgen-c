# uniffi-bindgen-c

[![CI](https://github.com/sorinirimies/uniffi-bindgen-c/actions/workflows/ci.yml/badge.svg)](https://github.com/sorinirimies/uniffi-bindgen-c/actions/workflows/ci.yml)
[![Security](https://github.com/sorinirimies/uniffi-bindgen-c/actions/workflows/security.yml/badge.svg)](https://github.com/sorinirimies/uniffi-bindgen-c/actions/workflows/security.yml)
[![Fuzz](https://github.com/sorinirimies/uniffi-bindgen-c/actions/workflows/fuzz.yml/badge.svg)](https://github.com/sorinirimies/uniffi-bindgen-c/actions/workflows/fuzz.yml)
[![crates.io](https://img.shields.io/crates/v/uniffi-bindgen-c.svg)](https://crates.io/crates/uniffi-bindgen-c)

C11 bindings generator for [UniFFI](https://mozilla.github.io/uniffi-rs/) 0.32.

Point it at a library built with UniFFI and it writes a header and a source file per
component, plus one shared header-only runtime. The generated code calls the scaffolding
through its C ABI, so you link the Rust library like any other native library — and so
does anything that speaks C: Zig, Nim, Odin, Fortran, Julia, embedded toolchains.

* **Complete:** generates and compiles for every fixture of uniffi's own test suite
  (functions, records, enums, errors, objects, traits, callback interfaces, async, custom
  and external types, renames, docstrings) as C11 with `-Wall -Wextra -Wpedantic -Werror`,
  with GCC and Clang, and under ASan/UBSan; every header also compiles as C++17.
* **Plain C:** structs, enums, tagged unions, value handles, explicit `*_free`, and a
  status out-parameter instead of exceptions. No hidden allocator, no globals you manage.
* **Callbacks and traits in C:** implement a callback interface or a Rust trait with a
  vtable of function pointers and a `void *ctx`.
* **Async:** every async function has a blocking form and an `_async` form that calls you
  back on completion.
* **Hardened decoding:** what comes from Rust is bounds-checked; the generated decoders
  are fuzzed (libFuzzer, ASan, UBSan) to round-trip byte for byte.

## Install

```sh
cargo install uniffi-bindgen-c
```

or download the archive for your platform from the
[latest release](https://github.com/sorinirimies/uniffi-bindgen-c/releases).

| uniffi-bindgen-c | UniFFI |
|---|---|
| 0.1 | 0.32 |

A UniFFI minor version can change the FFI, so use the release that matches the `uniffi`
version of the Rust library. A library built with another version fails at generation.

## Use

```sh
cargo build --release                       # your crate, as a cdylib or staticlib
uniffi-bindgen-c generate target/release/libmylib.so --out-dir bindings/
cc -std=c11 app.c bindings/mylib.c -Ibindings -Ltarget/release -lmylib -lpthread
```

`generate` also takes a `.udl` file (`--library` names the compiled library, `--crate` the
crate). `uniffi-bindgen-c runtime <dir>` writes just `uniffi_runtime.h`.

### Configuration

In the crate's `uniffi.toml`:

```toml
[bindings.c]
prefix = "mylib"          # prefix of every generated name; default: the UniFFI namespace
omit_checksums = false    # skip the API checksum check at start-up
cdylib_name = "mylib"     # informational

[bindings.c.rename]       # C names only, in UniFFI's dot notation
OldRecord = "NewRecord"
"OldRecord.field" = "new_field"
```

## The generated API

Names are `<prefix>_<Name>` for types and `<prefix>_<function>` for functions; methods
and constructors are `<prefix>_<Type>_<name>`. Every call takes a trailing
`uniffi_status *status`.

| Rust | C |
|---|---|
| `u8`…`i64`, `f32`, `f64`, `bool` | `uint8_t`…`int64_t`, `float`, `double`, `bool` |
| `String` | `uniffi_string` (`char *data; size_t len;`, NUL-terminated when owned) |
| `Vec<u8>` / `&[u8]` | `uniffi_bytes`; `&[u8]` arguments are lent to Rust without a copy |
| `SystemTime`, `Duration` | `uniffi_timestamp`, `uniffi_duration` (seconds + nanoseconds) |
| `Option<T>` | `<prefix>_OptionT` (`bool has_value; T value;`) |
| `Vec<T>`, `HashSet<T>` | `<prefix>_SequenceT`, `<prefix>_SetT` (`T *items; size_t len;`) |
| `HashMap<K, V>` | `<prefix>_MapKV` (`K *keys; V *values; size_t len;`) |
| `Box<T>` | `T *` (recursive records and enums) |
| record | `struct`; `<prefix>_<Record>_DEFAULTS` initializer when fields have defaults |
| enum without data | `enum` (Rust's discriminants) |
| enum with data, error | `struct { tag; union { struct {...} Variant; ... } u; }` |
| flat error | `struct { tag; uniffi_string message; }` |
| object | `struct { uint64_t handle; }`, a value handle: `_clone`, `_free` |
| callback interface | `struct { const vtable *vtable; void *ctx; }` |
| custom type | `typedef` of its builtin type |
| `Display`, `Debug`, `Eq`, `Hash`, `Ord` | `_to_string`, `_debug_string`, `_eq`/`_ne`, `_hash`, `_cmp` |

```c
#include "tour.h"

uniffi_status st = {0};
tour_Counter counter = tour_Counter_new(10, &st);
int64_t n = tour_Counter_add(counter, 5, &st);          /* 15 */
uniffi_string s = tour_Counter_to_string(counter, &st); /* "Counter(15)" */
uniffi_string_free(&s);
tour_Counter_free(counter);
```

### Ownership

* **Arguments are borrowed.** The bindings copy what they need during the call, so pass
  literals and stack memory freely: `tour_shout(uniffi_str("hi"), NULL)`.
* **Results are owned** by the caller: free them with `uniffi_string_free`,
  `uniffi_bytes_free` or the type's `<prefix>_<Type>_free`. Freeing a record, enum or
  container frees everything inside it. Objects are released with `<Type>_free`; a
  `_clone` is a second handle, released separately.

### Errors

Pass `NULL` as `status` to abort the process (with the message on stderr) on any error,
or a zero-initialized `uniffi_status` to inspect `status.code` after the call:
`UNIFFI_OK`, `UNIFFI_ERROR` (the declared error type), `UNIFFI_PANIC`,
`UNIFFI_CANCELLED` or `UNIFFI_INTERNAL`. `status.message` describes the failure; free it
with `uniffi_status_free` (a later successful call clears it).

A function that returns `Result<_, E>` takes an `E *err` before the status. On
`UNIFFI_ERROR` it holds the error, which you free; pass `NULL` to ignore it.

```c
tour_DataError err;
uniffi_status st = {0};
tour_fail_data(1, &err, &st);
if (st.code == UNIFFI_ERROR && err.tag == tour_DataError_BadCode) {
    printf("%d (%s)\n", err.u.BadCode.code, st.message.data);
    tour_DataError_free(&err);
}
uniffi_status_free(&st);
```

### Callback interfaces and traits

Fill a vtable with your functions; each gets your `ctx` first, the arguments (borrowed),
then the error out-parameter and status for reporting failures. Return values are handed
to the bindings.

```c
static int32_t on_event(void *ctx, uniffi_string name, int32_t value, uniffi_status *status) {
    return value * 10;
}
static void on_done(void *ctx, uniffi_status *status) {}
static const tour_Listener_vtable vtable = {on_event, on_done, fallible};

tour_Listener listener = {&vtable, &my_state};    /* ctx must outlive Rust's use of it */
tour_notify(listener, 4, NULL);
```

A Rust trait that C may implement (`#[uniffi::export(with_foreign)]`) is an object:
`<Type>_from_impl(&vtable, ctx)` makes one, and its vtable's `uniffi_release(ctx)` runs
once neither C nor Rust holds a handle any more.

Async trait methods are implemented as plain blocking functions; Rust's call runs them
on a thread of their own.

### Async

`tour_slow_add(a, b, &st)` blocks until Rust's future completes.
`tour_slow_add_async(a, b, done, ctx)` returns at once and calls
`done(ctx, result, status)` on another thread when it completes; `done` owns `result`,
while `err` and `status` are valid during the call only.

## Limits

* Defaults of function arguments do not exist in C: pass every argument.
* Custom types are `typedef`s of their builtin type; converting functions are not
  generated.
* Windows is not yet tested in CI (the runtime uses Win32 threads there).
* `_async` calls use one thread each while they wait.

## Development

Everything is scripted in Nushell and wired through `just`:

```sh
just              # list recipes
just check-all    # fmt, clippy, tests, docs, Nushell script tests
just e2e          # generate, compile and run the tour fixture's C test (--sanitize)
just fixtures     # uniffi's fixture suite: generate, compile as C11 and C++17 (--sanitize)
just fuzz-c       # fuzz the generated decoders (libFuzzer if clang has it)
just vendor-fixtures ~/uniffi-rs v0.32.2   # re-vendor uniffi's fixtures
just release 0.1.0       # bump, changelog, tag, push → Release workflow
just release-all 0.1.0   # … to GitHub and Gitea
```

Layout:

```
crates/uniffi-bindgen-c/   the crate: codegen/ (the generator), runtime/ (the C runtime header), cli, config; tests/
fixtures/tour/    a UniFFI crate that uses every feature, with the C test that runs against it (just e2e)
fixtures/uniffi/  uniffi-rs v0.32.2 fixtures and examples (scripts/vendor_fixtures.nu), compiled by just fixtures
fuzz/             cargo-fuzz targets (nightly), and fuzz/c: the generated decoders under libFuzzer
scripts/          all tooling, Nushell only (justfile recipes run in nu too)
```

## License

MIT
