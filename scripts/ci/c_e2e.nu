#!/usr/bin/env nu
# ── uniffi-bindgen-c · C end-to-end test ───────────────────────────────────
#
# Builds the `tour` fixture crate, generates C bindings for it, compiles them
# as C11 (warnings are errors), checks the header compiles as C++17, and runs
# fixtures/tour/c/test_tour.c.
#
# Usage: nu scripts/ci/c_e2e.nu [--sanitize]

const FIXTURE = "fixtures/tour"

use cc.nu

# Arguments that compile the generated bindings and the test into `output`.
export def compile_args [out_dir: string, lib_dir: string, output: string, sanitize: bool, os: string] {
    [
        ...(cc flags $sanitize)
        $"($out_dir)/tour.c" $"($out_dir)/test_tour.c"
        $"-I($out_dir)"
        ...(cc link_args $lib_dir "tour" $os)
        "-o" $output
    ]
}

def main [--sanitize] {
    let root = ($env.FILE_PWD | path dirname | path dirname)
    cd $root
    print -e "▶ building the tour fixture"
    ^cargo build --manifest-path $"($FIXTURE)/Cargo.toml"
    let lib_dir = ($root | path join $FIXTURE target debug)
    let lib = ($lib_dir | path join (cc library_file "tour" $nu.os-info.name))
    let out_dir = (mktemp --directory)

    print -e "▶ generating bindings"
    ^cargo run --quiet -p uniffi-bindgen-c -- generate $lib --out-dir $out_dir --no-format --metadata-no-deps
    cp $"($FIXTURE)/c/test_tour.c" $out_dir

    let compiler = (cc compiler ($env.CC? | default "") (cc available))
    let output = ($out_dir | path join test_tour)
    print -e $"▶ compiling with ($compiler)"
    ^$compiler ...(compile_args $out_dir $lib_dir $output $sanitize $nu.os-info.name)
    let cxx = (cc cxx_compiler ($env.CXX? | default ""))
    if $cxx != "" {
        print -e $"▶ checking the header as C++ with ($cxx)"
        ^$cxx ...(cc cxx_header_flags) $"-I($out_dir)" $"($out_dir)/tour.h"
    }

    print -e "▶ running"
    ^$output
    print -e "✔ C end-to-end test passed"
}
