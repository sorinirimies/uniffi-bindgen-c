#!/usr/bin/env nu
# ── uniffi-bindgen-c · uniffi fixture suite ────────────────────────────────
#
# Runs uniffi's own test fixtures (fixtures/uniffi, vendored from uniffi-rs)
# against the C bindings:
#   1. builds every fixture crate,
#   2. generates C for every cdylib, compiles it as C11 (warnings are errors) and
#      checks its header compiles as C++17,
#   3. compiles, links and runs each fixture's c/test_*.c.
#
# Usage: nu scripts/ci/fixtures.nu [--sanitize] [--only <cdylib name>]

use cc.nu

const FIXTURES = "fixtures/uniffi"

# Libraries we only build: upstream does not generate bindings for them either.
export const SKIP = [
    # Its UDL lives in another crate, so library mode cannot find the UDL types.
    "uniffi_proc_macro_nip"
]

# The cdylib targets in `cargo metadata` output: name and package directory.
export def cdylibs [metadata: record] {
    $metadata.packages | each { |p|
        $p.targets | where { |t| "cdylib" in $t.crate_types } | each { |t|
            {name: $t.name, dir: ($p.manifest_path | path dirname)}
        }
    } | flatten | sort-by name
}

# The libraries to run: all but SKIP, or just `only`.
export def selected [libs: list, only: string] {
    $libs | where { |l| $l.name not-in $SKIP and ($only == "" or $l.name == $only) }
}

def run_library [lib: record, bin: string, lib_dir: string, out_root: string, compiler: string, sanitize: bool] {
    let out = ($out_root | path join $lib.name)
    mkdir $out
    let library = ($lib_dir | path join (cc library_file $lib.name $nu.os-info.name))
    # Library mode finds UDL files through `cargo metadata`, so run inside the fixture workspace.
    let gen = (do { cd $lib.dir; ^$bin generate $library --out-dir $out --no-format } | complete)
    if $gen.exit_code != 0 { return [{library: $lib.name, step: "generate", detail: $gen.stderr}] }

    let flags = (cc flags $sanitize)
    let sources = (glob $"($out)/*.c")
    mut objects = []
    mut failures = []
    for source in $sources {
        let object = ($source | str replace -r '\.c$' '.o')
        let r = (do { ^$compiler ...$flags -c $source $"-I($out)" -o $object } | complete)
        if $r.exit_code != 0 {
            $failures = ($failures | append {library: $lib.name, step: $"compile ($source | path basename)", detail: $r.stderr})
        }
        $objects = ($objects | append $object)
    }
    let cxx = (cc cxx_compiler ($env.CXX? | default ""))
    if $cxx != "" {
        for header in (glob $"($out)/*.h" | where { |h| ($h | path basename) != "uniffi_runtime.h" }) {
            let r = (do { ^$cxx ...(cc cxx_header_flags) $"-I($out)" $header } | complete)
            if $r.exit_code != 0 {
                $failures = ($failures | append {library: $lib.name, step: $"C++ check ($header | path basename)", detail: $r.stderr})
            }
        }
    }
    if ($failures | is-not-empty) { return $failures }
    let objects = $objects

    let check_dir = ($env.FILE_PWD | path dirname | path dirname | path join $FIXTURES c)
    for test in (glob $"($lib.dir)/c/test_*.c") {
        let exe = ($out | path join ($test | path parse | get stem))
        let r = (do { ^$compiler ...$flags $test ...$objects $"-I($out)" $"-I($check_dir)" ...(cc link_args $lib_dir $lib.name $nu.os-info.name) -o $exe } | complete)
        if $r.exit_code != 0 {
            $failures = ($failures | append {library: $lib.name, step: $"build ($test | path basename)", detail: $r.stderr})
            continue
        }
        let run = (do { ^$exe } | complete)
        print -e $"  ($lib.name)/($test | path basename): ($run.stdout | str trim)"
        if $run.exit_code != 0 {
            $failures = ($failures | append {library: $lib.name, step: $"run ($test | path basename)", detail: $"($run.stdout)($run.stderr)"})
        }
    }
    $failures
}

def main [--sanitize, --only: string = ""] {
    let root = ($env.FILE_PWD | path dirname | path dirname)
    cd $root
    let manifest = ($FIXTURES | path join Cargo.toml)
    print -e "▶ building the fixtures"
    ^cargo build --quiet --manifest-path $manifest
    ^cargo build --quiet -p uniffi-bindgen-c
    let bin = ($root | path join target debug uniffi-bindgen-c)
    let lib_dir = ($root | path join $FIXTURES target debug)
    let libs = (selected (cdylibs (^cargo metadata --format-version 1 --no-deps --manifest-path $manifest | from json)) $only)
    let compiler = (cc compiler ($env.CC? | default "") (cc available))
    let out_root = (mktemp --directory)

    print -e $"▶ generating, compiling and testing ($libs | length) libraries with ($compiler)"
    let failures = ($libs | each { |lib| run_library $lib $bin $lib_dir $out_root $compiler $sanitize } | flatten)
    if ($failures | is-not-empty) {
        for f in $failures {
            print -e $"✗ ($f.library): ($f.step)\n($f.detail | lines | first 30 | str join "\n")"
        }
        error make {msg: $"($failures | length) fixture steps failed"}
    }
    print -e "✔ all fixtures passed"
}
