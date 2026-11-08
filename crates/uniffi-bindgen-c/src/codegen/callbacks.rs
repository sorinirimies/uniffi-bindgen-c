//! C implementations of callback interfaces and traits: the handle maps, and the
//! vtable Rust calls back through.

use std::fmt::Write;

use uniffi_bindgen::interface::{AsType, FfiCallbackFunction, Method};

use super::names::escape;
use super::types::Gen;

impl Gen<'_> {
    /// The handle map and record type of a C-implementable interface, plus the
    /// function that releases a record.
    pub(crate) fn emit_handle_map(&self, name: &str, releasable: bool) -> String {
        let release = if releasable {
            "    if (rec->vtable->uniffi_release) {\n        rec->vtable->uniffi_release(rec->ctx);\n    }\n"
        } else {
            ""
        };
        format!(
            "typedef struct {name}_uniffi_record {{\n    const {name}_vtable *vtable;\n    void *ctx;\n}} {name}_uniffi_record;\n\nstatic uniffi_handle_map {name}_uniffi_handles = UNIFFI_HANDLE_MAP_INIT;\n\nstatic void {name}_uniffi_drop(uint64_t handle) {{\n    {name}_uniffi_record *rec = ({name}_uniffi_record *)uniffi_handle_map_remove(&{name}_uniffi_handles, handle);\n    if (!rec) {{\n        return;\n    }}\n{release}    free(rec);\n}}\n\nstatic uint64_t {name}_uniffi_insert(const {name}_vtable *vtable, void *ctx) {{\n    {name}_uniffi_record *rec = ({name}_uniffi_record *)uniffi_alloc(sizeof *rec);\n    rec->vtable = vtable;\n    rec->ctx = ctx;\n    return uniffi_handle_map_insert(&{name}_uniffi_handles, rec);\n}}\n\n"
        )
    }

    /// Lowering and lifting a callback interface value (a vtable and a context).
    pub(crate) fn emit_callback_lift_lower(&self, name: &str) -> String {
        format!(
            "uint64_t {name}_uniffi_lower({name} value) {{\n    return {name}_uniffi_insert(value.vtable, value.ctx);\n}}\n\n{name} {name}_uniffi_lift(uint64_t handle) {{\n    {name} value = {{NULL, NULL}};\n    {name}_uniffi_record *rec = ({name}_uniffi_record *)uniffi_handle_map_get(&{name}_uniffi_handles, handle);\n    if (rec) {{\n        value.vtable = rec->vtable;\n        value.ctx = rec->ctx;\n    }}\n    {name}_uniffi_drop(handle);\n    return value;\n}}\n\n"
        )
    }

    fn vtable_params(&self, callback: &FfiCallbackFunction, method: &Method) -> String {
        let n_args = method.arguments().len();
        let mut params: Vec<String> = callback
            .arguments()
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let name = match i {
                    0 => "uniffi_handle".to_owned(),
                    i if i <= n_args => format!("arg{}", i - 1),
                    _ => a.name().to_owned(),
                };
                format!("{} {name}", self.ffi_type(&a.type_()))
            })
            .collect();
        if callback.has_rust_call_status_arg() {
            params.push("RustCallStatus *uniffi_call_status".into());
        }
        params.join(", ")
    }

    /// Lifts the FFI arguments `arg<i>` of `m` into `a<i>` (prefixed by `holder`).
    fn lift_args(&self, m: &Method, holder: &str, declare: bool) -> String {
        let mut out = String::new();
        for (i, a) in m.arguments().iter().enumerate() {
            let t = a.as_type();
            let decl = if declare {
                format!("{} ", self.c_type(&t))
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "    {decl}{holder}a{i} = {};",
                self.lift(&t, &format!("arg{i}"))
            );
        }
        out
    }

    fn free_args(&self, m: &Method, holder: &str) -> String {
        let mut out = String::new();
        for (i, a) in m.arguments().iter().enumerate() {
            if let Some(stmt) = self.free_stmt(&a.as_type(), &format!("{holder}a{i}")) {
                let _ = writeln!(out, "    {stmt}");
            }
        }
        out
    }

    /// Calls the C function and turns its outcome into `status_lvalue` (a
    /// RustCallStatus), storing the lowered result with `store`.
    fn invoke(
        &self,
        name: &str,
        m: &Method,
        holder: &str,
        status_lvalue: &str,
        store: &dyn Fn(&str) -> String,
    ) -> String {
        let mut out = String::new();
        let mut args = vec!["rec->ctx".to_owned()];
        args.extend((0..m.arguments().len()).map(|i| format!("{holder}a{i}")));
        if let Some(t) = m.throws_type() {
            let ct = self.c_type(t);
            let _ = writeln!(
                out,
                "    {ct} uniffi_err;\n    memset(&uniffi_err, 0, sizeof uniffi_err);"
            );
            args.push("&uniffi_err".to_owned());
        }
        let _ = writeln!(
            out,
            "    uniffi_status uniffi_status_value = {{0, {{NULL, 0}}}};"
        );
        args.push("&uniffi_status_value".to_owned());
        let call = format!("rec->vtable->{}({})", escape(m.name()), args.join(", "));
        match m.return_type() {
            Some(t) => {
                let _ = writeln!(out, "    {} uniffi_result = {call};", self.c_type(t));
            }
            None => {
                let _ = writeln!(out, "    {call};");
            }
        }
        let _ = writeln!(out, "    if (uniffi_status_value.code == UNIFFI_OK) {{");
        if let Some(t) = m.return_type() {
            let _ = writeln!(out, "        {}", store(&self.lower(t, "uniffi_result")));
            if let Some(stmt) = self.free_stmt(t, "uniffi_result") {
                let _ = writeln!(out, "        {stmt}");
            }
        }
        let _ = writeln!(out, "    }}");
        if let Some(t) = m.throws_type() {
            let free = self.free_stmt(t, "uniffi_err").unwrap_or_default();
            let _ = writeln!(
                out,
                "    else if (uniffi_status_value.code == UNIFFI_ERROR) {{\n        uniffi_writer w = {{NULL, 0, 0}};\n        {}(&w, uniffi_err);\n        {status_lvalue}.code = UNIFFI_CALL_ERROR;\n        {status_lvalue}.error_buf = uniffi_buffer_from(w.data, w.len);\n        uniffi_writer_free(&w);\n        {free}\n    }}",
                self.write_fn(t)
            );
        }
        let _ = writeln!(
            out,
            "    else {{\n        const char *fallback = \"the C implementation of {name} failed\";\n        uniffi_string text = uniffi_status_value.message;\n        {status_lvalue}.code = UNIFFI_CALL_UNEXPECTED_ERROR;\n        {status_lvalue}.error_buf = text.len ? uniffi_buffer_from(text.data, text.len) : uniffi_buffer_from(fallback, strlen(fallback));\n    }}\n    uniffi_status_free(&uniffi_status_value);"
        );
        out
    }

    fn sync_wrapper(
        &self,
        name: &str,
        i: usize,
        callback: &FfiCallbackFunction,
        m: &Method,
    ) -> String {
        let store = |lowered: &str| format!("*uniffi_out_return = {lowered};");
        let void_out = if m.return_type().is_none() {
            "    (void)uniffi_out_return;\n"
        } else {
            ""
        };
        format!(
            "static void {name}_uniffi_m{i}({params}) {{\n{void_out}    {name}_uniffi_record *rec = ({name}_uniffi_record *)uniffi_handle_map_get(&{name}_uniffi_handles, uniffi_handle);\n    if (!rec) {{\n        const char *gone = \"the object was released already\";\n        uniffi_call_status->code = UNIFFI_CALL_UNEXPECTED_ERROR;\n        uniffi_call_status->error_buf = uniffi_buffer_from(gone, strlen(gone));\n        return;\n    }}\n{lift}{invoke}\n{free}}}\n\n",
            params = self.vtable_params(callback, m),
            lift = self.lift_args(m, "", true),
            invoke = self.invoke(name, m, "", "(*uniffi_call_status)", &store),
            free = self.free_args(m, ""),
        )
    }

    fn async_wrapper(
        &self,
        name: &str,
        i: usize,
        callback: &FfiCallbackFunction,
        m: &Method,
    ) -> String {
        let job = format!("{name}_uniffi_m{i}_job");
        let result_struct = m.foreign_future_ffi_result_struct().name().to_owned();
        let complete_type = callback
            .arguments()
            .iter()
            .find(|a| a.name() == "uniffi_future_callback")
            .map(|a| self.ffi_type(&a.type_()))
            .unwrap_or_else(|| "void *".to_owned());
        let mut fields = String::new();
        for (j, a) in m.arguments().iter().enumerate() {
            let _ = writeln!(fields, "    {} a{j};", self.c_type(&a.as_type()));
        }
        let store = |lowered: &str| format!("result.return_value = {lowered};");
        format!(
            "typedef struct {job} {{\n    uint64_t handle;\n{fields}    {complete_type} complete;\n    uint64_t data;\n}} {job};\n\nstatic void {job}_run(void *raw) {{\n    {job} *job = ({job} *)raw;\n    {result_struct} result;\n    memset(&result, 0, sizeof result);\n    {name}_uniffi_record *rec = ({name}_uniffi_record *)uniffi_handle_map_get(&{name}_uniffi_handles, job->handle);\n    if (rec) {{\n{invoke}\n    }} else {{\n        const char *gone = \"the object was released already\";\n        result.call_status.code = UNIFFI_CALL_UNEXPECTED_ERROR;\n        result.call_status.error_buf = uniffi_buffer_from(gone, strlen(gone));\n    }}\n{free}    {name}_uniffi_drop(job->handle);\n    job->complete(job->data, result);\n    free(job);\n}}\n\nstatic void {name}_uniffi_m{i}({params}) {{\n    uniffi_out_dropped_callback->handle = 0;\n    uniffi_out_dropped_callback->free = uniffi_foreign_future_dropped;\n    {job} *job = ({job} *)uniffi_alloc(sizeof *job);\n    job->handle = uniffi_handle_map_clone(&{name}_uniffi_handles, uniffi_handle);\n{lift}    job->complete = uniffi_future_callback;\n    job->data = uniffi_callback_data;\n    uniffi_spawn({job}_run, job);\n}}\n\n",
            params = self.vtable_params(callback, m),
            invoke = self
                .invoke(name, m, "job->", "result.call_status", &store)
                .replace("\n    ", "\n        "),
            free = self.free_args(m, "job->"),
            lift = self.lift_args(m, "job->", false),
        )
    }

    /// The wrappers of every method and the function registering the vtable with Rust.
    pub(crate) fn emit_callback_vtable(
        &self,
        name: &str,
        methods: &[(FfiCallbackFunction, Method)],
        init_fn: &str,
        vtable_type: &str,
    ) -> String {
        let mut out = format!(
            "static void {name}_uniffi_free(uint64_t handle) {{\n    {name}_uniffi_drop(handle);\n}}\n\nstatic uint64_t {name}_uniffi_clone(uint64_t handle) {{\n    return uniffi_handle_map_clone(&{name}_uniffi_handles, handle);\n}}\n\n"
        );
        let mut entries = vec![
            format!("{name}_uniffi_free"),
            format!("{name}_uniffi_clone"),
        ];
        for (i, (callback, m)) in methods.iter().enumerate() {
            if m.is_async() {
                out.push_str(&self.async_wrapper(name, i, callback, m));
            } else {
                out.push_str(&self.sync_wrapper(name, i, callback, m));
            }
            entries.push(format!("{name}_uniffi_m{i}"));
        }
        let _ = writeln!(
            out,
            "static void {name}_uniffi_init(void) {{\n    static {vtable_type} vtable = {{{}}};\n    {init_fn}(&vtable);\n}}\n",
            entries.join(", ")
        );
        out
    }

    /// Whether any C-implemented method is async (then the dropped-future no-op is needed).
    pub(crate) fn has_async_callbacks(&self) -> bool {
        self.ci
            .callback_interface_definitions()
            .iter()
            .any(|cb| cb.methods().iter().any(|m| m.is_async()))
            || self
                .ci
                .object_definitions()
                .iter()
                .any(|o| o.has_callback_interface() && o.methods().iter().any(|m| m.is_async()))
    }
}
