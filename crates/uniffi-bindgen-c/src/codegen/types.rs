//! Mapping UniFFI types to C: type names, mangled names of generic containers,
//! FFI types, lifting and lowering.

use std::collections::BTreeSet;

use uniffi_bindgen::ComponentInterface;
use uniffi_bindgen::interface::{AsType, DefaultValue, FfiType, Literal, Radix, Type};

use super::names::{escape, string_literal};
use crate::config::Config;

/// Everything the emitters need to know about the component being generated.
#[derive(Debug)]
pub(crate) struct Gen<'a> {
    pub(crate) ci: &'a ComponentInterface,
    pub(crate) config: &'a Config,
    /// Prefix of every public name: `<prefix>_<Type>`, `<prefix>_<function>`.
    pub(crate) prefix: String,
    /// Names of the error types of the other components generated alongside this one.
    external_errors: &'a BTreeSet<String>,
}

impl<'a> Gen<'a> {
    pub(crate) fn new(
        ci: &'a ComponentInterface,
        config: &'a Config,
        external_errors: &'a BTreeSet<String>,
    ) -> Self {
        Self {
            ci,
            config,
            prefix: config.prefix_for(ci.namespace()),
            external_errors,
        }
    }

    /// Every type the component uses, nested ones included, each once. UniFFI's type
    /// universe misses some nested types (`Option<Box<T>>`), so walk the definitions too.
    pub(crate) fn all_types(&self) -> Vec<Type> {
        let mut roots: Vec<Type> = self
            .ci
            .iter_local_types()
            .chain(self.ci.iter_external_types())
            .cloned()
            .collect();
        for r in self.ci.record_definitions() {
            roots.extend(r.fields().iter().map(|f| f.as_type()));
        }
        for e in self.ci.enum_definitions() {
            for v in e.variants() {
                roots.extend(v.fields().iter().map(|f| f.as_type()));
            }
        }
        for c in self.all_callables() {
            roots.extend(c.arguments().iter().map(|a| a.as_type()));
            roots.extend(c.return_type().cloned());
            roots.extend(c.throws_type().cloned());
        }
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for t in &roots {
            for inner in t.iter_types() {
                // One C type per C name: the same custom type can reach here through
                // two module paths.
                if seen.insert(self.mangle(inner)) {
                    out.push(inner.clone());
                }
            }
        }
        out
    }

    /// The prefix of the component that defines `t` (its namespace when external).
    pub(crate) fn owner_prefix(&self, t: &Type) -> String {
        if !self.ci.is_external(t) {
            return self.prefix.clone();
        }
        match self.ci.namespace_for_type(t) {
            Ok(ns) => ns.to_owned(),
            Err(_) => module_path_of(t)
                .map(|p| p.split("::").next().unwrap_or(p).to_owned())
                .unwrap_or_else(|| self.prefix.clone()),
        }
    }

    /// `name` after the `rename` table (UniFFI does not rename every reference).
    pub(crate) fn renamed_type<'n>(&'n self, name: &'n str) -> &'n str {
        self.config
            .rename
            .get(name)
            .and_then(|v| v.as_str())
            .unwrap_or(name)
    }

    /// The public C name of a user-defined type: `<owner prefix>_<Name>`.
    pub(crate) fn named(&self, t: &Type, name: &str) -> String {
        format!(
            "{}_{}",
            self.owner_prefix(t),
            escape(self.renamed_type(name))
        )
    }

    /// The public C name of a type this component defines.
    pub(crate) fn local(&self, name: &str) -> String {
        format!("{}_{}", self.prefix, escape(name))
    }

    /// A public function or constant of this component: `<prefix>_<name>`.
    pub(crate) fn public(&self, name: &str) -> String {
        format!("{}_{name}", self.prefix)
    }

    /// The C type that holds a value of `t`.
    pub(crate) fn c_type(&self, t: &Type) -> String {
        match t {
            Type::UInt8 => "uint8_t".into(),
            Type::Int8 => "int8_t".into(),
            Type::UInt16 => "uint16_t".into(),
            Type::Int16 => "int16_t".into(),
            Type::UInt32 => "uint32_t".into(),
            Type::Int32 => "int32_t".into(),
            Type::UInt64 => "uint64_t".into(),
            Type::Int64 => "int64_t".into(),
            Type::Float32 => "float".into(),
            Type::Float64 => "double".into(),
            Type::Boolean => "bool".into(),
            Type::String => "uniffi_string".into(),
            Type::Bytes => "uniffi_bytes".into(),
            Type::Timestamp => "uniffi_timestamp".into(),
            Type::Duration => "uniffi_duration".into(),
            Type::Object { name, .. }
            | Type::CallbackInterface { name, .. }
            | Type::Record { name, .. }
            | Type::Enum { name, .. }
            | Type::Custom { name, .. } => self.named(t, name),
            Type::Box { inner_type } => format!("{} *", self.c_type(inner_type)),
            Type::Optional { .. } | Type::Sequence { .. } | Type::Set { .. } | Type::Map { .. } => {
                format!("{}_{}", self.prefix, self.mangle(t))
            }
        }
    }

    /// A name for `t` that is a valid C identifier and unique among the types of a
    /// component: `String`, `OptionString`, `SequenceU32`, `MapStringRecord`.
    pub(crate) fn mangle(&self, t: &Type) -> String {
        match t {
            Type::UInt8 => "U8".into(),
            Type::Int8 => "I8".into(),
            Type::UInt16 => "U16".into(),
            Type::Int16 => "I16".into(),
            Type::UInt32 => "U32".into(),
            Type::Int32 => "I32".into(),
            Type::UInt64 => "U64".into(),
            Type::Int64 => "I64".into(),
            Type::Float32 => "F32".into(),
            Type::Float64 => "F64".into(),
            Type::Boolean => "Bool".into(),
            Type::String => "String".into(),
            Type::Bytes => "Bytes".into(),
            Type::Timestamp => "Timestamp".into(),
            Type::Duration => "Duration".into(),
            Type::Object { name, .. }
            | Type::CallbackInterface { name, .. }
            | Type::Record { name, .. }
            | Type::Enum { name, .. }
            | Type::Custom { name, .. } => {
                let name = escape(self.renamed_type(name));
                if self.ci.is_external(t) {
                    format!("{}_{name}", self.owner_prefix(t))
                } else {
                    name
                }
            }
            Type::Box { inner_type } => format!("Box{}", self.mangle(inner_type)),
            Type::Optional { inner_type } => format!("Option{}", self.mangle(inner_type)),
            Type::Sequence { inner_type } => format!("Sequence{}", self.mangle(inner_type)),
            Type::Set { inner_type } => format!("Set{}", self.mangle(inner_type)),
            Type::Map {
                key_type,
                value_type,
            } => format!("Map{}{}", self.mangle(key_type), self.mangle(value_type)),
        }
    }

    /// Whether `t` is a generic container this component defines a struct for.
    pub(crate) fn is_container(t: &Type) -> bool {
        matches!(
            t,
            Type::Optional { .. }
                | Type::Sequence { .. }
                | Type::Set { .. }
                | Type::Map { .. }
                | Type::Box { .. }
        )
    }

    /// The function writing a `t` into a `uniffi_writer`: `fn(uniffi_writer *, T)`.
    pub(crate) fn write_fn(&self, t: &Type) -> String {
        match primitive_codec(t) {
            Some(p) => format!("uniffi_write_{p}"),
            None if Self::is_container(t) => {
                format!("{}_uniffi_write_{}", self.prefix, self.mangle(t))
            }
            None => format!(
                "{}_uniffi_write_{}",
                self.owner_prefix(t),
                self.local_mangle(t)
            ),
        }
    }

    /// The function reading a `t` from a `uniffi_reader`: `T fn(uniffi_reader *)`.
    pub(crate) fn read_fn(&self, t: &Type) -> String {
        match primitive_codec(t) {
            Some(p) => format!("uniffi_read_{p}"),
            None if Self::is_container(t) => {
                format!("{}_uniffi_read_{}", self.prefix, self.mangle(t))
            }
            None => format!(
                "{}_uniffi_read_{}",
                self.owner_prefix(t),
                self.local_mangle(t)
            ),
        }
    }

    /// The name of a user-defined type as its own component mangles it.
    fn local_mangle(&self, t: &Type) -> String {
        match t {
            Type::Object { name, .. }
            | Type::CallbackInterface { name, .. }
            | Type::Record { name, .. }
            | Type::Enum { name, .. }
            | Type::Custom { name, .. } => escape(self.renamed_type(name)),
            other => self.mangle(other),
        }
    }

    /// A statement freeing the value `expr` (an lvalue) of type `t`, or `None` when a
    /// `t` owns no memory.
    pub(crate) fn free_stmt(&self, t: &Type, expr: &str) -> Option<String> {
        match t {
            Type::String => Some(format!("uniffi_string_free(&{expr});")),
            Type::Bytes => Some(format!("uniffi_bytes_free(&{expr});")),
            Type::Object { .. } => Some(format!("{}_free({expr});", self.c_type(t))),
            Type::CallbackInterface { .. } => None,
            // Every enum has a `_free`, a no-op for enums without data: an external
            // enum's shape is not known here.
            Type::Record { .. } | Type::Enum { .. } => {
                Some(format!("{}_free(&{expr});", self.c_type(t)))
            }
            Type::Custom { builtin, .. } => self.free_stmt(builtin, expr),
            Type::Optional { .. }
            | Type::Sequence { .. }
            | Type::Set { .. }
            | Type::Map { .. }
            | Type::Box { .. } => {
                Some(format!("{}_{}_free(&{expr});", self.prefix, self.mangle(t)))
            }
            _ => None,
        }
    }

    /// Whether `t` is an enum without data that is not an error: a C `enum`.
    pub(crate) fn is_flat_enum(&self, t: &Type) -> bool {
        match t {
            Type::Enum { name, .. } => {
                let local = self.ci.get_enum_definition(name);
                match local {
                    Some(e) => e.is_flat() && !self.is_error_enum(name),
                    None => false,
                }
            }
            _ => false,
        }
    }

    pub(crate) fn is_error_enum(&self, name: &str) -> bool {
        self.ci.get_object_definition(name).is_none()
            && self.ci.get_record_definition(name).is_none()
            && (self.ci.is_name_used_as_error(name) || self.external_errors.contains(name))
    }

    /// The C spelling of an FFI-level type.
    pub(crate) fn ffi_type(&self, t: &FfiType) -> String {
        match t {
            FfiType::UInt8 => "uint8_t".into(),
            FfiType::Int8 => "int8_t".into(),
            FfiType::UInt16 => "uint16_t".into(),
            FfiType::Int16 => "int16_t".into(),
            FfiType::UInt32 => "uint32_t".into(),
            FfiType::Int32 => "int32_t".into(),
            FfiType::UInt64 | FfiType::Handle => "uint64_t".into(),
            FfiType::Int64 => "int64_t".into(),
            FfiType::Float32 => "float".into(),
            FfiType::Float64 => "double".into(),
            FfiType::RustBuffer(_) => "RustBuffer".into(),
            FfiType::ForeignBytes => "ForeignBytes".into(),
            FfiType::Callback(name) | FfiType::Struct(name) => name.clone(),
            FfiType::RustCallStatus => "RustCallStatus".into(),
            FfiType::Reference(inner) => format!("const {} *", self.ffi_type(inner)),
            FfiType::MutReference(inner) => format!("{} *", self.ffi_type(inner)),
            FfiType::VoidPointer => "void *".into(),
        }
    }

    /// Expression lowering the C value `expr` of type `t` to its FFI value.
    pub(crate) fn lower(&self, t: &Type, expr: &str) -> String {
        match t {
            Type::Boolean => format!("(int8_t)({expr} ? 1 : 0)"),
            Type::UInt8
            | Type::Int8
            | Type::UInt16
            | Type::Int16
            | Type::UInt32
            | Type::Int32
            | Type::UInt64
            | Type::Int64
            | Type::Float32
            | Type::Float64 => expr.to_owned(),
            Type::Object { .. } | Type::CallbackInterface { .. } => {
                format!("{}_uniffi_lower({expr})", self.c_type(t))
            }
            Type::Custom { builtin, .. } => self.lower(builtin, expr),
            _ => format!("{}_uniffi_lower_{}({expr})", self.prefix, self.mangle(t)),
        }
    }

    /// Expression lifting the FFI value `expr` to the C type of `t`.
    pub(crate) fn lift(&self, t: &Type, expr: &str) -> String {
        match t {
            Type::Boolean => format!("({expr} != 0)"),
            Type::UInt8
            | Type::Int8
            | Type::UInt16
            | Type::Int16
            | Type::UInt32
            | Type::Int32
            | Type::UInt64
            | Type::Int64
            | Type::Float32
            | Type::Float64 => expr.to_owned(),
            Type::Object { .. } => format!("{}_uniffi_lift({expr})", self.c_type(t)),
            Type::CallbackInterface { .. } => format!("{}_uniffi_lift({expr})", self.c_type(t)),
            Type::Custom { builtin, .. } => self.lift(builtin, expr),
            _ => format!("{}_uniffi_lift_{}({expr})", self.prefix, self.mangle(t)),
        }
    }

    /// Whether a value of `t` crosses the FFI in a RustBuffer.
    pub(crate) fn is_buffer_type(t: &Type) -> bool {
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
            | Type::Float64
            | Type::Object { .. }
            | Type::CallbackInterface { .. } => false,
            Type::Custom { builtin, .. } => Self::is_buffer_type(builtin),
            _ => true,
        }
    }

    /// The smallest number of bytes a `t` takes in a buffer; 0 when it can be empty.
    pub(crate) fn min_wire_size(&self, t: &Type) -> usize {
        match t {
            Type::Boolean | Type::UInt8 | Type::Int8 | Type::Optional { .. } => 1,
            Type::UInt16 | Type::Int16 => 2,
            Type::UInt32 | Type::Int32 | Type::Float32 => 4,
            Type::UInt64 | Type::Int64 | Type::Float64 => 8,
            Type::String
            | Type::Bytes
            | Type::Sequence { .. }
            | Type::Set { .. }
            | Type::Map { .. }
            | Type::Enum { .. } => 4,
            Type::Timestamp | Type::Duration => 12,
            Type::Object { .. } | Type::CallbackInterface { .. } => 8,
            Type::Box { inner_type } => self.min_wire_size(inner_type),
            Type::Custom { builtin, .. } => self.min_wire_size(builtin),
            Type::Record { name, .. } => self.ci.get_record_definition(name).map_or(0, |r| {
                r.fields()
                    .iter()
                    .map(|f| self.min_wire_size(&f.as_type()))
                    .sum()
            }),
        }
    }

    /// An initializer for a default argument or field value, or `None` when the value
    /// has no constant form in C.
    pub(crate) fn default_value(&self, value: &DefaultValue, t: &Type) -> Option<String> {
        match value {
            DefaultValue::Default => Some(self.zero(t)),
            DefaultValue::Literal(lit) => self.literal(lit, t),
        }
    }

    /// The zero value of `t` as an initializer.
    pub(crate) fn zero(&self, t: &Type) -> String {
        match t {
            Type::Boolean => "false".into(),
            Type::UInt8
            | Type::Int8
            | Type::UInt16
            | Type::Int16
            | Type::UInt32
            | Type::Int32
            | Type::UInt64
            | Type::Int64 => "0".into(),
            Type::Float32 | Type::Float64 => "0.0".into(),
            Type::Box { .. } => "NULL".into(),
            Type::Enum { .. } if self.is_flat_enum(t) => format!("({})0", self.c_type(t)),
            _ => "{0}".into(),
        }
    }

    fn literal(&self, lit: &Literal, t: &Type) -> Option<String> {
        Some(match lit {
            Literal::Boolean(b) => b.to_string(),
            Literal::String(s) => format!("{{(char *){}, {}}}", string_literal(s), s.len()),
            Literal::UInt(v, radix, ty) => int_literal(*v as i128, *radix, ty),
            Literal::Int(v, radix, ty) => int_literal(i128::from(*v), *radix, ty),
            Literal::Float(s, ty) => match ty {
                Type::Float32 => format!("{s}f"),
                _ => s.clone(),
            },
            Literal::Enum(variant, ty) => match ty {
                Type::Enum { name, .. } if self.is_flat_enum(ty) => {
                    format!(
                        "{}_{}",
                        self.named(ty, name),
                        escape(&self.variant_named(name, variant))
                    )
                }
                _ => return None,
            },
            Literal::EmptySequence | Literal::EmptyMap | Literal::EmptySet => "{0}".into(),
            Literal::None => "{0}".into(),
            Literal::Some { inner } => {
                let inner_type = match t {
                    Type::Optional { inner_type } => inner_type.as_ref(),
                    other => other,
                };
                let inner = self.default_value(inner, inner_type)?;
                format!("{{true, {inner}}}")
            }
        })
    }

    /// The variant of enum `enum_name` a default value refers to. UDL spells it in any
    /// case (`"DEUX"` for `Deux`), so match ignoring case and underscores.
    pub(crate) fn variant_named(&self, enum_name: &str, literal: &str) -> String {
        let normal = |s: &str| s.replace('_', "").to_ascii_lowercase();
        self.ci
            .get_enum_definition(enum_name)
            .and_then(|e| {
                e.variants()
                    .iter()
                    .find(|v| normal(v.name()) == normal(literal))
            })
            .map_or_else(|| literal.to_owned(), |v| v.name().to_owned())
    }
}

/// The runtime codec suffix of a type the runtime handles itself.
fn primitive_codec(t: &Type) -> Option<&'static str> {
    Some(match t {
        Type::UInt8 => "u8",
        Type::Int8 => "i8",
        Type::UInt16 => "u16",
        Type::Int16 => "i16",
        Type::UInt32 => "u32",
        Type::Int32 => "i32",
        Type::UInt64 => "u64",
        Type::Int64 => "i64",
        Type::Float32 => "f32",
        Type::Float64 => "f64",
        Type::Boolean => "bool",
        Type::String => "string",
        Type::Bytes => "bytes",
        Type::Timestamp => "timestamp",
        Type::Duration => "duration",
        _ => return None,
    })
}

fn int_literal(v: i128, radix: Radix, ty: &Type) -> String {
    let suffix = match ty {
        Type::UInt64 => "ULL",
        Type::Int64 => "LL",
        Type::UInt32 => "U",
        _ => "",
    };
    if v == i128::from(i64::MIN) {
        // The minimum of a signed type cannot be written as a literal.
        return "INT64_MIN".into();
    }
    if v < 0 {
        return format!("({v}{suffix})");
    }
    match radix {
        Radix::Decimal => format!("{v}{suffix}"),
        Radix::Octal => format!("0{v:o}{suffix}"),
        Radix::Hexadecimal => format!("0x{v:x}{suffix}"),
    }
}

fn module_path_of(t: &Type) -> Option<&str> {
    match t {
        Type::Object { module_path, .. }
        | Type::Record { module_path, .. }
        | Type::Enum { module_path, .. }
        | Type::CallbackInterface { module_path, .. }
        | Type::Custom { module_path, .. } => Some(module_path),
        _ => None,
    }
}
