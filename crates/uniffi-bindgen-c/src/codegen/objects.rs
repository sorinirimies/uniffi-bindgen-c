//! Objects (handles), and the methods and constructors of objects, records and enums.

use std::fmt::Write;

use uniffi_bindgen::interface::{Constructor, Method, Object, Type, UniffiTraitMethods};

use super::calls::Receiver;
use super::header::method_name;
use super::names::{escape, var};
use super::types::Gen;

impl Gen<'_> {
    pub(crate) fn emit_object(&self, obj: &Object) -> String {
        let name = self.local(obj.name());
        let foreign = obj.has_callback_interface();
        let mut out = String::new();
        let foreign_free = if foreign {
            format!(
                "    if (self.handle & 1) {{\n        {name}_uniffi_drop(self.handle);\n        return;\n    }}\n"
            )
        } else {
            String::new()
        };
        let foreign_clone = if foreign {
            format!(
                "    if (self.handle & 1) {{\n        out.handle = uniffi_handle_map_clone(&{name}_uniffi_handles, self.handle);\n        return out;\n    }}\n"
            )
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            "void {name}_free({name} self) {{\n    if (self.handle == 0) {{\n        return;\n    }}\n{foreign_free}    RustCallStatus uniffi_call = {{0}};\n    {free}(self.handle, &uniffi_call);\n}}\n\n{name} {name}_clone({name} self) {{\n    {name} out = {{0}};\n    if (self.handle == 0) {{\n        return out;\n    }}\n{foreign_clone}    {prefix}_uniffi_ensure_init();\n    RustCallStatus uniffi_call = {{0}};\n    out.handle = {clone}(self.handle, &uniffi_call);\n    uniffi_check_call(&uniffi_call, NULL, NULL, NULL);\n    return out;\n}}\n\nuint64_t {name}_uniffi_lower({name} value) {{\n    return {name}_clone(value).handle;\n}}\n\n{name} {name}_uniffi_lift(uint64_t handle) {{\n    {name} value;\n    value.handle = handle;\n    return value;\n}}\n",
            free = obj.ffi_object_free().name(),
            clone = obj.ffi_object_clone().name(),
            prefix = self.prefix,
        );
        if foreign {
            let _ = writeln!(
                out,
                "{name} {name}_from_impl(const {name}_vtable *vtable, void *ctx) {{\n    {name} value;\n    value.handle = {name}_uniffi_insert(vtable, ctx);\n    return value;\n}}\n"
            );
        }
        let receiver = Receiver::Handle(format!("{name}_uniffi_lower(self)"));
        for c in obj.constructors() {
            out.push_str(&self.constructor(&name, c));
        }
        for m in obj.methods() {
            let fname = format!("{name}_{}", method_name(m.name()));
            let foreign_call = if foreign {
                self.foreign_dispatch(&name, m)
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "{} {{\n{foreign_call}{}}}\n",
                self.signature(m, &fname, Some(format!("{name} self"))),
                self.call_body(m, &receiver)
            );
            if m.is_async() {
                out.push_str(&self.async_variant(
                    m,
                    &fname,
                    Some(format!("{name} self")),
                    &receiver,
                ));
            }
        }
        out.push_str(&self.trait_methods(&name, &obj.uniffi_trait_methods(), &receiver));
        out
    }

    /// Calls the C implementation directly when `self` is one (an odd handle).
    fn foreign_dispatch(&self, name: &str, m: &Method) -> String {
        let mut args = vec!["rec->ctx".to_owned()];
        args.extend(m.arguments().iter().map(|a| var(a.name())));
        let mut out = format!(
            "    if (self.handle & 1) {{\n        {name}_uniffi_record *rec = ({name}_uniffi_record *)uniffi_handle_map_get(&{name}_uniffi_handles, self.handle);\n        uniffi_status local = {{0, {{NULL, 0}}}};\n"
        );
        if let Some(t) = m.throws_type() {
            let ct = self.c_type(t);
            let _ = writeln!(
                out,
                "        {ct} local_err;\n        memset(&local_err, 0, sizeof local_err);"
            );
            args.push("&local_err".to_owned());
        }
        args.push("&local".to_owned());
        let ret = self.return_c_type(m);
        let call = format!("rec->vtable->{}({})", escape(m.name()), args.join(", "));
        let (assign, ret_stmt) = if m.return_type().is_some() {
            (
                format!("{ret} result = "),
                "        return result;\n".to_owned(),
            )
        } else {
            (String::new(), "        return;\n".to_owned())
        };
        let zero = if m.return_type().is_some() {
            format!(
                "            {ret} zero;\n            memset(&zero, 0, sizeof zero);\n            return zero;\n"
            )
        } else {
            "            return;\n".to_owned()
        };
        let _ = writeln!(
            out,
            "        if (!rec) {{\n            uniffi_fail_str(status, UNIFFI_INTERNAL, \"the object was released already\");\n{zero}        }}\n        {assign}{call};"
        );
        if let Some(t) = m.throws_type() {
            let free = self.free_stmt(t, "local_err").unwrap_or_default();
            let _ = writeln!(
                out,
                "        if (local.code == UNIFFI_ERROR) {{\n            if (err) {{\n                *err = local_err;\n            }} else {{\n                {free}\n            }}\n        }}"
            );
        }
        let _ = writeln!(
            out,
            "        if (local.code != UNIFFI_OK) {{\n            uniffi_fail(status, local.code, local.message);\n        }} else {{\n            uniffi_status_ok(status);\n        }}\n{ret_stmt}    }}"
        );
        out
    }

    fn constructor(&self, type_name: &str, c: &Constructor) -> String {
        let fname = format!("{type_name}_{}", method_name(c.name()));
        let mut out = format!(
            "{} {{\n{}}}\n\n",
            self.signature(c, &fname, None),
            self.call_body(c, &Receiver::None)
        );
        if c.is_async() {
            out.push_str(&self.async_variant(c, &fname, None, &Receiver::None));
        }
        out
    }

    /// Constructors, methods and trait methods of a record or enum.
    pub(crate) fn emit_value_methods(
        &self,
        t: &Type,
        constructors: &[Constructor],
        methods: &[Method],
        traits: &UniffiTraitMethods,
    ) -> String {
        let name = self.c_type(t);
        let receiver = Receiver::Value(t.clone());
        let mut out = String::new();
        for c in constructors {
            out.push_str(&self.constructor(&name, c));
        }
        for m in methods {
            let fname = format!("{name}_{}", method_name(m.name()));
            let _ = writeln!(
                out,
                "{} {{\n{}}}\n",
                self.signature(m, &fname, Some(format!("{name} self"))),
                self.call_body(m, &receiver)
            );
            if m.is_async() {
                out.push_str(&self.async_variant(
                    m,
                    &fname,
                    Some(format!("{name} self")),
                    &receiver,
                ));
            }
        }
        out.push_str(&self.trait_methods(&name, traits, &receiver));
        out
    }

    fn trait_methods(
        &self,
        name: &str,
        traits: &UniffiTraitMethods,
        receiver: &Receiver,
    ) -> String {
        let mut out = String::new();
        let entries: [(&Option<Method>, &str, &str, bool); 6] = [
            (&traits.display_fmt, "to_string", "uniffi_string", false),
            (&traits.debug_fmt, "debug_string", "uniffi_string", false),
            (&traits.eq_eq, "eq", "bool", true),
            (&traits.eq_ne, "ne", "bool", true),
            (&traits.hash_hash, "hash", "uint64_t", false),
            (&traits.ord_cmp, "cmp", "int8_t", true),
        ];
        for (method, suffix, ret, binary) in entries {
            let Some(m) = method else { continue };
            let other = if binary {
                format!(", {name} other")
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "{ret} {name}_{suffix}({name} self{other}, uniffi_status *status) {{\n{}}}\n",
                self.call_body(m, receiver)
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn method_names_escape_reserved_suffixes() {
        assert_eq!(super::method_name("free"), "free_");
        assert_eq!(super::method_name("connect"), "connect");
    }
}
