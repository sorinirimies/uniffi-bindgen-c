//! Serialization of every type into UniFFI's buffers, releasing values, and
//! lowering/lifting buffer-borne values across the FFI.

use std::collections::BTreeSet;
use std::fmt::Write;

use uniffi_bindgen::interface::{AsType, Enum, Record, Type};

use super::names::{escape, field, var};
use super::types::Gen;

impl Gen<'_> {
    /// Prototypes of the container codecs (static) and of the lowering/lifting helpers.
    pub(crate) fn emit_codec_prototypes(&self) -> String {
        let mut out = String::from("/* ---- Serialization ---- */\n\n");
        for t in self.containers() {
            let ct = self.c_type(&t);
            let _ = writeln!(
                out,
                "static UNIFFI_MAYBE_UNUSED void {}(uniffi_writer *w, {ct} value);\nstatic UNIFFI_MAYBE_UNUSED {ct} {}(uniffi_reader *r);",
                self.write_fn(&t),
                self.read_fn(&t)
            );
        }
        for t in self.buffer_types() {
            let ct = self.c_type(&t);
            let m = self.mangle(&t);
            let _ = writeln!(
                out,
                "static UNIFFI_MAYBE_UNUSED RustBuffer {p}_uniffi_lower_{m}({ct} value);\nstatic UNIFFI_MAYBE_UNUSED {ct} {p}_uniffi_lift_{m}(RustBuffer buffer);",
                p = self.prefix
            );
        }
        out.push('\n');
        out
    }

    /// Every type that crosses the FFI in a buffer somewhere in this component.
    fn buffer_types(&self) -> Vec<Type> {
        let mut seen = BTreeSet::new();
        self.all_types()
            .iter()
            .filter(|t| Self::is_buffer_type(t))
            .filter(|t| seen.insert(self.mangle(t)))
            .cloned()
            .collect()
    }

    pub(crate) fn emit_codecs(&self) -> String {
        let mut out = String::new();
        for r in self.ci.record_definitions() {
            out.push_str(&self.record_codec(r));
        }
        for e in self.ci.enum_definitions() {
            out.push_str(&self.enum_codec(e));
        }
        for t in self.all_types().iter() {
            if let Type::Custom { name, builtin, .. } = t
                && !self.ci.is_external(t)
            {
                let ct = self.local(name);
                let _ = writeln!(
                    out,
                    "void {}(uniffi_writer *w, {ct} value) {{\n    {}(w, value);\n}}\n\n{ct} {}(uniffi_reader *r) {{\n    return {}(r);\n}}\n",
                    self.write_fn(t),
                    self.write_fn(builtin),
                    self.read_fn(t),
                    self.read_fn(builtin)
                );
            }
        }
        for obj in self.ci.object_definitions() {
            out.push_str(&self.handle_codec(&obj.as_type()));
        }
        for cb in self.ci.callback_interface_definitions() {
            out.push_str(&self.handle_codec(&cb.as_type()));
        }
        for t in self.containers() {
            out.push_str(&self.container_codec(&t));
        }
        for t in self.buffer_types() {
            out.push_str(&self.lower_lift(&t));
        }
        out
    }

    fn handle_codec(&self, t: &Type) -> String {
        let ct = self.c_type(t);
        format!(
            "void {}(uniffi_writer *w, {ct} value) {{\n    uniffi_write_u64(w, {ct}_uniffi_lower(value));\n}}\n\n{ct} {}(uniffi_reader *r) {{\n    return {ct}_uniffi_lift(uniffi_read_u64(r));\n}}\n\n",
            self.write_fn(t),
            self.read_fn(t)
        )
    }

    fn record_codec(&self, r: &Record) -> String {
        let t = r.as_type();
        let name = self.local(r.name());
        let (mut write, mut read, mut free) = (String::new(), String::new(), String::new());
        for f in r.fields() {
            let ft = f.as_type();
            let fname = var(f.name());
            let _ = writeln!(write, "    {}(w, value.{fname});", self.write_fn(&ft));
            let _ = writeln!(read, "    value.{fname} = {}(r);", self.read_fn(&ft));
            if let Some(stmt) = self.free_stmt(&ft, &format!("value->{fname}")) {
                let _ = writeln!(free, "    {stmt}");
            }
        }
        format!(
            "void {w}(uniffi_writer *w, {name} value) {{\n    (void)w;\n    (void)value;\n{write}}}\n\n{name} {rd}(uniffi_reader *r) {{\n    {name} value;\n    memset(&value, 0, sizeof value);\n    (void)r;\n{read}    return value;\n}}\n\nvoid {name}_free({name} *value) {{\n    if (!value) {{\n        return;\n    }}\n{free}    memset(value, 0, sizeof *value);\n}}\n\n",
            w = self.write_fn(&t),
            rd = self.read_fn(&t),
        )
    }

    fn enum_codec(&self, e: &Enum) -> String {
        let t = e.as_type();
        let name = self.local(e.name());
        let w = self.write_fn(&t);
        let rd = self.read_fn(&t);
        if self.is_flat_enum(&t) {
            let mut write = String::new();
            let mut read = String::new();
            for (i, v) in e.variants().iter().enumerate() {
                let c = format!("{name}_{}", escape(v.name()));
                let _ = writeln!(
                    write,
                    "        case {c}: uniffi_write_i32(w, {}); return;",
                    i + 1
                );
                let _ = writeln!(read, "        case {}: return {c};", i + 1);
            }
            let first = e.variants().first().map_or_else(
                || format!("({name})0"),
                |v| format!("{name}_{}", escape(v.name())),
            );
            return format!(
                "void {w}(uniffi_writer *w, {name} value) {{\n    switch (value) {{\n{write}    }}\n    uniffi_write_i32(w, 0);\n}}\n\n{name} {rd}(uniffi_reader *r) {{\n    switch (uniffi_read_i32(r)) {{\n{read}        default: r->failed = true; return {first};\n    }}\n}}\n\nvoid {name}_free({name} *value) {{\n    (void)value;\n}}\n\n"
            );
        }
        let flat_error = e.is_flat();
        let (mut write, mut read, mut free) = (String::new(), String::new(), String::new());
        for (i, v) in e.variants().iter().enumerate() {
            let tag = format!("{name}_{}", escape(v.name()));
            let vname = escape(v.name());
            let _ = writeln!(
                write,
                "        case {tag}:\n            uniffi_write_i32(w, {});",
                i + 1
            );
            let _ = writeln!(
                read,
                "        case {}:\n            value.tag = {tag};",
                i + 1
            );
            if flat_error {
                let _ = writeln!(read, "            value.message = uniffi_read_string(r);");
            } else {
                let _ = writeln!(free, "        case {tag}:");
                for (j, f) in v.fields().iter().enumerate() {
                    let ft = f.as_type();
                    let fname = field(f.name(), j);
                    let _ = writeln!(
                        write,
                        "            {}(w, value.u.{vname}.{fname});",
                        self.write_fn(&ft)
                    );
                    let _ = writeln!(
                        read,
                        "            value.u.{vname}.{fname} = {}(r);",
                        self.read_fn(&ft)
                    );
                    if let Some(stmt) = self.free_stmt(&ft, &format!("value->u.{vname}.{fname}")) {
                        let _ = writeln!(free, "            {stmt}");
                    }
                }
                let _ = writeln!(free, "            break;");
            }
            let _ = writeln!(write, "            return;");
            let _ = writeln!(read, "            return value;");
        }
        let free_body = if flat_error {
            "    uniffi_string_free(&value->message);\n".to_owned()
        } else {
            format!(
                "    switch (value->tag) {{\n{free}        default:\n            break;\n    }}\n"
            )
        };
        let mut names = String::new();
        for v in e.variants() {
            let _ = write!(names, "\"{}\", ", v.name());
        }
        let variant_name = if self.is_error_enum(e.name()) {
            format!(
                "/* The variant name of an error, for its message. */\nstatic UNIFFI_MAYBE_UNUSED const char *{name}_uniffi_variant_name({name} value) {{\n    static const char *const names[] = {{{names}NULL}};\n    return ((size_t)value.tag < sizeof names / sizeof names[0] - 1) ? names[value.tag] : \"{}\";\n}}\n\n",
                e.name()
            )
        } else {
            String::new()
        };
        format!(
            "void {w}(uniffi_writer *w, {name} value) {{\n    switch (value.tag) {{\n{write}        default:\n            break;\n    }}\n    uniffi_write_i32(w, 0);\n}}\n\n{name} {rd}(uniffi_reader *r) {{\n    {name} value;\n    memset(&value, 0, sizeof value);\n    switch (uniffi_read_i32(r)) {{\n{read}        default:\n            r->failed = true;\n            return value;\n    }}\n}}\n\nvoid {name}_free({name} *value) {{\n    if (!value) {{\n        return;\n    }}\n{free_body}    memset(value, 0, sizeof *value);\n}}\n\n{variant_name}"
        )
    }

    fn container_codec(&self, t: &Type) -> String {
        let ct = self.c_type(t);
        let w = self.write_fn(t);
        let rd = self.read_fn(t);
        let free_name = format!("{}_{}_free", self.prefix, self.mangle(t));
        match t {
            Type::Optional { inner_type } => {
                let inner = inner_type.as_ref();
                let free_inner = self
                    .free_stmt(inner, "value->value")
                    .map(|s| format!("    if (value->has_value) {{\n        {s}\n    }}\n"))
                    .unwrap_or_default();
                format!(
                    "static UNIFFI_MAYBE_UNUSED void {w}(uniffi_writer *w, {ct} value) {{\n    uniffi_write_i8(w, value.has_value ? 1 : 0);\n    if (value.has_value) {{\n        {}(w, value.value);\n    }}\n}}\n\nstatic UNIFFI_MAYBE_UNUSED {ct} {rd}(uniffi_reader *r) {{\n    {ct} value;\n    memset(&value, 0, sizeof value);\n    switch (uniffi_read_i8(r)) {{\n        case 0:\n            break;\n        case 1:\n            value.has_value = true;\n            value.value = {}(r);\n            break;\n        default:\n            r->failed = true;\n    }}\n    return value;\n}}\n\nvoid {free_name}({ct} *value) {{\n    if (!value) {{\n        return;\n    }}\n{free_inner}    memset(value, 0, sizeof *value);\n}}\n\n",
                    self.write_fn(inner),
                    self.read_fn(inner)
                )
            }
            Type::Sequence { inner_type } | Type::Set { inner_type } => {
                let inner = inner_type.as_ref();
                let it = self.c_type(inner);
                let free_item = self
                    .free_stmt(inner, "value->items[i]")
                    .map(|s| {
                        format!(
                            "    for (size_t i = 0; i < value->len; ++i) {{\n        {s}\n    }}\n"
                        )
                    })
                    .unwrap_or_default();
                format!(
                    "static UNIFFI_MAYBE_UNUSED void {w}(uniffi_writer *w, {ct} value) {{\n    uniffi_write_len(w, value.len);\n    for (size_t i = 0; i < value.len; ++i) {{\n        {}(w, value.items[i]);\n    }}\n}}\n\nstatic UNIFFI_MAYBE_UNUSED {ct} {rd}(uniffi_reader *r) {{\n    {ct} value;\n    memset(&value, 0, sizeof value);\n    size_t len = uniffi_read_len(r, {min});\n    if (r->failed) {{\n        return value;\n    }}\n    value.items = ({it} *)uniffi_alloc(len * sizeof *value.items);\n    for (size_t i = 0; i < len && !r->failed; ++i) {{\n        value.items[i] = {}(r);\n        value.len = i + 1;\n    }}\n    return value;\n}}\n\nvoid {free_name}({ct} *value) {{\n    if (!value) {{\n        return;\n    }}\n{free_item}    free(value->items);\n    memset(value, 0, sizeof *value);\n}}\n\n",
                    self.write_fn(inner),
                    self.read_fn(inner),
                    min = self.min_wire_size(inner)
                )
            }
            Type::Map {
                key_type,
                value_type,
            } => {
                let (k, v) = (key_type.as_ref(), value_type.as_ref());
                let mut free_items = String::new();
                if let Some(s) = self.free_stmt(k, "value->keys[i]") {
                    free_items.push_str(&format!("        {s}\n"));
                }
                if let Some(s) = self.free_stmt(v, "value->values[i]") {
                    free_items.push_str(&format!("        {s}\n"));
                }
                let free_loop = if free_items.is_empty() {
                    String::new()
                } else {
                    format!("    for (size_t i = 0; i < value->len; ++i) {{\n{free_items}    }}\n")
                };
                format!(
                    "static UNIFFI_MAYBE_UNUSED void {w}(uniffi_writer *w, {ct} value) {{\n    uniffi_write_len(w, value.len);\n    for (size_t i = 0; i < value.len; ++i) {{\n        {wk}(w, value.keys[i]);\n        {wv}(w, value.values[i]);\n    }}\n}}\n\nstatic UNIFFI_MAYBE_UNUSED {ct} {rd}(uniffi_reader *r) {{\n    {ct} value;\n    memset(&value, 0, sizeof value);\n    size_t len = uniffi_read_len(r, {min});\n    if (r->failed) {{\n        return value;\n    }}\n    value.keys = ({kt} *)uniffi_alloc(len * sizeof *value.keys);\n    value.values = ({vt} *)uniffi_alloc(len * sizeof *value.values);\n    for (size_t i = 0; i < len && !r->failed; ++i) {{\n        value.keys[i] = {rk}(r);\n        value.values[i] = {rv}(r);\n        value.len = i + 1;\n    }}\n    return value;\n}}\n\nvoid {free_name}({ct} *value) {{\n    if (!value) {{\n        return;\n    }}\n{free_loop}    free(value->keys);\n    free(value->values);\n    memset(value, 0, sizeof *value);\n}}\n\n",
                    wk = self.write_fn(k),
                    wv = self.write_fn(v),
                    rk = self.read_fn(k),
                    rv = self.read_fn(v),
                    kt = self.c_type(k),
                    vt = self.c_type(v),
                    min = self.min_wire_size(k) + self.min_wire_size(v),
                )
            }
            Type::Box { inner_type } => {
                let inner = inner_type.as_ref();
                let it = self.c_type(inner);
                let free_inner = self
                    .free_stmt(inner, "(**value)")
                    .map(|s| format!("        {s}\n"))
                    .unwrap_or_default();
                format!(
                    "static UNIFFI_MAYBE_UNUSED void {w}(uniffi_writer *w, {ct} value) {{\n    if (!value) {{\n        fputs(\"uniffi: a NULL box cannot be passed to Rust\\n\", stderr);\n        abort();\n    }}\n    {}(w, *value);\n}}\n\nstatic UNIFFI_MAYBE_UNUSED {ct} {rd}(uniffi_reader *r) {{\n    {it} *value = ({it} *)uniffi_alloc(sizeof *value);\n    *value = {}(r);\n    return value;\n}}\n\nvoid {free_name}({ct} *value) {{\n    if (!value || !*value) {{\n        return;\n    }}\n{free_inner}    free(*value);\n    *value = NULL;\n}}\n\n",
                    self.write_fn(inner),
                    self.read_fn(inner)
                )
            }
            _ => String::new(),
        }
    }

    /// Lowering into and lifting out of a RustBuffer. A string travels as its bare
    /// UTF-8 bytes, everything else in its serialized form.
    fn lower_lift(&self, t: &Type) -> String {
        let ct = self.c_type(t);
        let p = &self.prefix;
        let m = self.mangle(t);
        let is_string = matches!(t, Type::String)
            || matches!(t, Type::Custom { builtin, .. } if matches!(**builtin, Type::String));
        if is_string {
            return format!(
                "static UNIFFI_MAYBE_UNUSED RustBuffer {p}_uniffi_lower_{m}({ct} value) {{\n    return uniffi_buffer_from(value.data, value.len);\n}}\n\nstatic UNIFFI_MAYBE_UNUSED {ct} {p}_uniffi_lift_{m}(RustBuffer buffer) {{\n    {ct} value = uniffi_string_copy((const char *)buffer.data, (size_t)buffer.len);\n    uniffi_buffer_free(buffer);\n    return value;\n}}\n\n"
            );
        }
        format!(
            "static UNIFFI_MAYBE_UNUSED RustBuffer {p}_uniffi_lower_{m}({ct} value) {{\n    uniffi_writer w = {{NULL, 0, 0}};\n    {}(&w, value);\n    RustBuffer buffer = uniffi_buffer_from(w.data, w.len);\n    uniffi_writer_free(&w);\n    return buffer;\n}}\n\nstatic UNIFFI_MAYBE_UNUSED {ct} {p}_uniffi_lift_{m}(RustBuffer buffer) {{\n    uniffi_reader r = uniffi_reader_new(buffer.data, (size_t)buffer.len);\n    {ct} value = {}(&r);\n    bool bad = r.failed || uniffi_reader_remaining(&r) != 0;\n    uniffi_buffer_free(buffer);\n    if (bad) {{\n        fputs(\"uniffi: Rust sent a malformed {m}\\n\", stderr);\n        abort();\n    }}\n    return value;\n}}\n\n",
            self.write_fn(t),
            self.read_fn(t)
        )
    }
}
