//! Function bodies that call into Rust: synchronous calls, async calls (blocking,
//! and `_async` with a completion callback), and turning a call status into a
//! `uniffi_status` and a typed error.

use std::fmt::Write;

use uniffi_bindgen::interface::{AsType, Callable, Type};

use super::names::var;
use super::types::Gen;

/// How the receiver of a method reaches the FFI.
pub(crate) enum Receiver {
    /// A free function or constructor.
    None,
    /// An object: a fresh handle (`<Type>_uniffi_lower(self)`).
    Handle(String),
    /// A record or enum: the value, lowered into a buffer.
    Value(Type),
}

impl Gen<'_> {
    /// The name of the static function that lifts errors of `t` out of a buffer.
    pub(crate) fn error_lifter(&self, t: &Type) -> String {
        format!("{}_uniffi_lift_error_{}", self.prefix, self.mangle(t))
    }

    /// Error lifters for every type this component's callables throw.
    pub(crate) fn emit_error_lifters(&self) -> String {
        let mut out = String::new();
        let mut seen = std::collections::BTreeSet::new();
        for c in self.all_callables() {
            let Some(t) = c.throws_type() else { continue };
            if !seen.insert(self.mangle(t)) {
                continue;
            }
            let ct = self.c_type(t);
            let describe = match t {
                Type::Enum { name, .. } if !self.ci.is_external(t) && self.is_error_enum(name) => {
                    let flat = self
                        .ci
                        .get_enum_definition(name)
                        .is_some_and(|e| e.is_flat());
                    if flat {
                        format!(
                            "    const char *variant = {ct}_uniffi_variant_name(value);\n    size_t n = strlen(variant);\n    size_t total = n + 2 + value.message.len;\n    uniffi_string text;\n    text.data = (char *)uniffi_alloc(total + 1);\n    memcpy(text.data, variant, n);\n    text.data[n] = ':';\n    text.data[n + 1] = ' ';\n    if (value.message.len) {{\n        memcpy(text.data + n + 2, value.message.data, value.message.len);\n    }}\n    text.data[total] = '\\0';\n    text.len = total;\n    *message = text;\n"
                        )
                    } else {
                        format!(
                            "    const char *variant = {ct}_uniffi_variant_name(value);\n    *message = uniffi_string_copy(variant, strlen(variant));\n"
                        )
                    }
                }
                _ => {
                    let label = match t {
                        Type::Enum { name, .. } | Type::Object { name, .. } => name.clone(),
                        _ => "error".to_owned(),
                    };
                    format!(
                        "    *message = uniffi_string_copy(\"{label}\", {});\n",
                        label.len()
                    )
                }
            };
            let free = self
                .free_stmt(t, "value")
                .unwrap_or_else(|| "(void)value;".to_owned());
            let lift = if Self::is_buffer_type(t) {
                self.lift(t, "buffer")
            } else {
                // An object error travels as its handle, serialized.
                format!("{}(&r)", self.read_fn(t))
            };
            let body = if Self::is_buffer_type(t) {
                format!("    {ct} value = {lift};\n")
            } else {
                format!(
                    "    uniffi_reader r = uniffi_reader_new(buffer.data, (size_t)buffer.len);\n    {ct} value = {lift};\n    uniffi_buffer_free(buffer);\n"
                )
            };
            let _ = writeln!(
                out,
                "static UNIFFI_MAYBE_UNUSED void {}(RustBuffer buffer, void *out, uniffi_string *message) {{\n{body}{describe}    if (out) {{\n        *({ct} *)out = value;\n    }} else {{\n        {free}\n    }}\n}}\n",
                self.error_lifter(t)
            );
        }
        out
    }

    /// Every function, method and constructor of the component.
    pub(crate) fn all_callables(&self) -> Vec<&dyn Callable> {
        let mut out: Vec<&dyn Callable> = Vec::new();
        for f in self.ci.function_definitions() {
            out.push(f);
        }
        for o in self.ci.object_definitions() {
            for c in o.constructors() {
                out.push(c);
            }
            for m in o.methods() {
                out.push(m);
            }
        }
        for r in self.ci.record_definitions() {
            for c in r.constructors() {
                out.push(c);
            }
            for m in r.methods() {
                out.push(m);
            }
        }
        for e in self.ci.enum_definitions() {
            for c in e.constructors() {
                out.push(c);
            }
            for m in e.methods() {
                out.push(m);
            }
        }
        for cb in self.ci.callback_interface_definitions() {
            for m in cb.methods() {
                out.push(m);
            }
        }
        out
    }

    /// The FFI arguments of a call, lowered.
    fn lowered_args(&self, c: &dyn Callable, receiver: &Receiver) -> Vec<String> {
        let mut args = Vec::new();
        match receiver {
            Receiver::None => {}
            Receiver::Handle(lower) => args.push(lower.clone()),
            Receiver::Value(t) => args.push(self.lower(t, "self")),
        }
        for a in c.arguments() {
            let name = var(a.name());
            args.push(if a.is_borrowed_bytes() {
                format!("uniffi_borrow_bytes({name})")
            } else {
                self.lower(&a.as_type(), &name)
            });
        }
        args
    }

    /// `, <lifter>, err` or `, NULL, NULL` for `uniffi_check_call`.
    fn check_args(&self, c: &dyn Callable) -> String {
        match c.throws_type() {
            Some(t) => format!("{}, err", self.error_lifter(t)),
            None => "NULL, NULL".to_owned(),
        }
    }

    /// The zero value returned after a failed call.
    fn failure_return(&self, c: &dyn Callable) -> String {
        match c.return_type() {
            Some(t) => {
                let ct = self.c_type(t);
                match t {
                    Type::Boolean
                    | Type::UInt8
                    | Type::Int8
                    | Type::UInt16
                    | Type::Int16
                    | Type::UInt32
                    | Type::Int32
                    | Type::UInt64
                    | Type::Int64
                    | Type::Float32
                    | Type::Float64 => format!("return ({ct})0;"),
                    Type::Box { .. } => "return NULL;".to_owned(),
                    _ if self.is_flat_enum(t) => format!("return ({ct})0;"),
                    _ => format!(
                        "{{\n        {ct} uniffi_zero;\n        memset(&uniffi_zero, 0, sizeof uniffi_zero);\n        return uniffi_zero;\n    }}"
                    ),
                }
            }
            None => "return;".to_owned(),
        }
    }

    /// The body of a synchronous call, or of the blocking form of an async one.
    pub(crate) fn call_body(&self, c: &dyn Callable, receiver: &Receiver) -> String {
        let ffi_name = c.ffi_func().name().to_owned();
        let args = self.lowered_args(c, receiver);
        let mut out = format!("    {}_uniffi_ensure_init();\n", self.prefix);
        let check = self.check_args(c);
        let fail = self.failure_return(c);
        if c.is_async() {
            let _ = writeln!(
                out,
                "    uint64_t uniffi_future = {ffi_name}({});",
                args.join(", ")
            );
            let _ = writeln!(
                out,
                "    uniffi_future_block_on(uniffi_future, (uniffi_poll_fn){});",
                c.ffi_rust_future_poll(self.ci)
            );
            let _ = writeln!(out, "    RustCallStatus uniffi_call = {{0}};");
            let complete = c.ffi_rust_future_complete(self.ci);
            match c.return_type() {
                Some(t) => {
                    let _ = writeln!(
                        out,
                        "    {} uniffi_result = {complete}(uniffi_future, &uniffi_call);",
                        self.ffi_type(&t.into())
                    );
                    let _ = writeln!(
                        out,
                        "    {}(uniffi_future);",
                        c.ffi_rust_future_free(self.ci)
                    );
                    let _ = writeln!(
                        out,
                        "    if (!uniffi_check_call(&uniffi_call, status, {check})) {fail}"
                    );
                    let _ = writeln!(out, "    return {};", self.lift(t, "uniffi_result"));
                }
                None => {
                    let _ = writeln!(out, "    {complete}(uniffi_future, &uniffi_call);");
                    let _ = writeln!(
                        out,
                        "    {}(uniffi_future);",
                        c.ffi_rust_future_free(self.ci)
                    );
                    let _ = writeln!(out, "    uniffi_check_call(&uniffi_call, status, {check});");
                }
            }
            return out;
        }
        let mut all = args;
        all.push("&uniffi_call".to_owned());
        let call = format!("{ffi_name}({})", all.join(", "));
        let _ = writeln!(out, "    RustCallStatus uniffi_call = {{0}};");
        match c.return_type() {
            Some(t) => {
                let _ = writeln!(
                    out,
                    "    {} uniffi_result = {call};",
                    self.ffi_type(&t.into())
                );
                let _ = writeln!(
                    out,
                    "    if (!uniffi_check_call(&uniffi_call, status, {check})) {fail}"
                );
                let _ = writeln!(out, "    return {};", self.lift(t, "uniffi_result"));
            }
            None => {
                let _ = writeln!(out, "    {call};");
                let _ = writeln!(out, "    uniffi_check_call(&uniffi_call, status, {check});");
            }
        }
        out
    }

    /// The `_async` form: lowers the arguments now, then waits for the future and
    /// calls `done` on a new thread.
    pub(crate) fn async_variant(
        &self,
        c: &dyn Callable,
        fname: &str,
        receiver_param: Option<String>,
        receiver: &Receiver,
    ) -> String {
        let job = format!("{fname}_uniffi_job");
        let args = self.lowered_args(c, receiver);
        let check = match c.throws_type() {
            Some(t) => format!("{}, &uniffi_err", self.error_lifter(t)),
            None => "NULL, NULL".to_owned(),
        };
        let err_decl = c
            .throws_type()
            .map(|t| {
                let ct = self.c_type(t);
                format!("    {ct} uniffi_err;\n    memset(&uniffi_err, 0, sizeof uniffi_err);\n")
            })
            .unwrap_or_default();
        let err_arg = if c.throws_type().is_some() {
            ", &uniffi_err"
        } else {
            ""
        };
        let err_free = c
            .throws_type()
            .and_then(|t| self.free_stmt(t, "uniffi_err"))
            .map(|s| {
                format!(
                    "    if (uniffi_status_value.code == UNIFFI_ERROR) {{\n        {s}\n    }}\n"
                )
            })
            .unwrap_or_default();
        let complete = c.ffi_rust_future_complete(self.ci);
        let (get_result, call_done) = match c.return_type() {
            Some(t) => {
                let ct = self.c_type(t);
                (
                    format!(
                        "    {} uniffi_raw = {complete}(job->future, &uniffi_call);\n    {ct} uniffi_result;\n    memset(&uniffi_result, 0, sizeof uniffi_result);\n    if (uniffi_check_call(&uniffi_call, &uniffi_status_value, {check})) {{\n        uniffi_result = {};\n    }}\n",
                        self.ffi_type(&t.into()),
                        self.lift(t, "uniffi_raw")
                    ),
                    format!(
                        "    job->done(job->ctx, uniffi_result{err_arg}, &uniffi_status_value);\n"
                    ),
                )
            }
            None => (
                format!(
                    "    {complete}(job->future, &uniffi_call);\n    uniffi_check_call(&uniffi_call, &uniffi_status_value, {check});\n"
                ),
                format!("    job->done(job->ctx{err_arg}, &uniffi_status_value);\n"),
            ),
        };
        let mut params: Vec<String> = receiver_param.into_iter().collect();
        params.extend(self.params(c));
        params.push(self.done_param(c));
        params.push("void *ctx".to_owned());
        format!(
            "typedef struct {job} {{\n    uint64_t future;\n    {done};\n    void *ctx;\n}} {job};\n\nstatic void {job}_run(void *raw) {{\n    {job} *job = ({job} *)raw;\n    uniffi_future_block_on(job->future, (uniffi_poll_fn){poll});\n    RustCallStatus uniffi_call = {{0}};\n    uniffi_status uniffi_status_value = {{0, {{NULL, 0}}}};\n{err_decl}{get_result}    {free_fut}(job->future);\n{call_done}{err_free}    uniffi_status_free(&uniffi_status_value);\n    free(job);\n}}\n\nvoid {fname}_async({params}) {{\n    {prefix}_uniffi_ensure_init();\n    {job} *job = ({job} *)uniffi_alloc(sizeof *job);\n    job->future = {ffi}({args});\n    job->done = done;\n    job->ctx = ctx;\n    uniffi_spawn({job}_run, job);\n}}\n\n",
            done = self.done_param(c),
            poll = c.ffi_rust_future_poll(self.ci),
            free_fut = c.ffi_rust_future_free(self.ci),
            params = params.join(", "),
            prefix = self.prefix,
            ffi = c.ffi_func().name(),
            args = args.join(", "),
        )
    }
}
