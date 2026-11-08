#!/usr/bin/env nu
# ── uniffi-bindgen-c · fuzz the generated decoders ─────────────────────────
#
# Generates the tour fixture's bindings and fuzzes their decoding of values
# from Rust (fuzz/c/fuzz_codecs.c) under ASan and UBSan: with libFuzzer for
# --secs seconds when clang has it (Linux), else with the built-in
# pseudo-random driver.
#
# Usage: nu scripts/ci/fuzz_c.nu [--secs 60]

use cc.nu

const SOURCE = "fuzz/c/fuzz_codecs.c"
const FIXTURE = "fixtures/tour"

# Compiler arguments: libFuzzer, or the smoke driver.
export def build_args [libfuzzer: bool, gen_dir: string, lib_dir: string, output: string, os: string] {
    let fuzzer = if $libfuzzer { "-fsanitize=fuzzer,address,undefined" } else { "-fsanitize=address,undefined" }
    let smoke = if $libfuzzer { [] } else { ["-DUNIFFI_FUZZ_SMOKE"] }
    ["-std=c11" "-O1" "-g" $fuzzer "-fno-sanitize-recover=all" ...$smoke $"-I($gen_dir)" $SOURCE ...(cc link_args $lib_dir "tour" $os) "-o" $output]
}

# Arguments for running the fuzzer.
export def run_args [libfuzzer: bool, secs: int, corpus: string] {
    if $libfuzzer {
        [$corpus $"-max_total_time=($secs)" "-max_len=4096"]
    } else {
        ["1000000"]
    }
}

def main [--secs: int = 60] {
    let root = ($env.FILE_PWD | path dirname | path dirname)
    cd $root
    ^cargo build --quiet --manifest-path $"($FIXTURE)/Cargo.toml"
    let lib_dir = ($root | path join $FIXTURE target debug)
    let gen = (mktemp --directory)
    ^cargo run --quiet -p uniffi-bindgen-c -- generate ($lib_dir | path join (cc library_file "tour" $nu.os-info.name)) --out-dir $gen --no-format --metadata-no-deps
    let compiler = (cc compiler ($env.CC? | default "") (["clang"] | append (cc available) | uniq | where { |c| (which $c | is-not-empty) }))
    let exe = ($gen | path join fuzz_codecs)
    # libFuzzer ships with LLVM's clang, not with Apple's.
    let libfuzzer = (do { ^$compiler ...(build_args true $gen $lib_dir $exe $nu.os-info.name) } | complete).exit_code == 0
    if not $libfuzzer {
        print -e "▶ no libFuzzer: using the pseudo-random driver"
        ^$compiler ...(build_args false $gen $lib_dir $exe $nu.os-info.name)
    }
    let corpus = ($gen | path join corpus)
    mkdir $corpus
    ^$exe ...(run_args $libfuzzer $secs $corpus)
    print -e "✔ fuzzing the generated decoders found nothing"
}
