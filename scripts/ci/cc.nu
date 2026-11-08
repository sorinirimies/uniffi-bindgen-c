# ── uniffi-bindgen-c · C toolchain helpers ─────────────────────────────────
# Shared by c_e2e.nu and fixtures.nu: compiler choice, flags, library names.

# File name of the shared library `name` on `os`.
export def library_file [name: string, os: string] {
    match $os {
        "macos" => $"lib($name).dylib"
        "windows" => $"($name).dll"
        _ => $"lib($name).so"
    }
}

# The C compiler to use: $CC, else the first of cc, clang, gcc that exists.
export def compiler [cc: string, available: list<string>] {
    if $cc != "" { return $cc }
    let found = ["cc" "clang" "gcc"] | where { |c| $c in $available }
    if ($found | is-empty) { error make {msg: "no C compiler found (set $CC)"} }
    $found | first
}

# Flags every compile uses: C11, all warnings and pedantic, warnings are errors.
export def flags [sanitize: bool] {
    let sanitizers = if $sanitize { ["-fsanitize=address,undefined" "-fno-omit-frame-pointer"] } else { [] }
    ["-std=c11" "-Wall" "-Wextra" "-Wpedantic" "-Werror" "-g" ...$sanitizers]
}

# Flags that check a generated header compiles as C++ too (C++ code includes C headers).
export def cxx_header_flags [] {
    ["-x" "c++" "-std=c++17" "-Wall" "-Wextra" "-Werror" "-fsyntax-only"]
}

# Linker arguments for the Rust library `name` in `dir` (plus pthreads off macOS).
export def link_args [dir: string, name: string, os: string] {
    let threads = if $os in ["macos" "windows"] { [] } else { ["-lpthread"] }
    [$"-L($dir)" $"-l($name)" $"-Wl,-rpath,($dir)" ...$threads]
}

# The compilers that are installed.
export def available [] {
    ["cc" "clang" "gcc"] | where { |c| (which $c | is-not-empty) }
}

# The C++ compiler for the header check: $CXX, else c++ / clang++ / g++, else none.
export def cxx_compiler [cxx: string] {
    if $cxx != "" { return $cxx }
    ["c++" "clang++" "g++"] | where { |c| (which $c | is-not-empty) } | get 0? | default ""
}
