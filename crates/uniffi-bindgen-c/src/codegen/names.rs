//! Identifier handling: C (and C++) keywords, prefixes, templating helpers.
//!
//! Generated headers are valid C and C++, so names avoid both languages' keywords.

const KEYWORDS: &[&str] = &[
    "alignas",
    "alignof",
    "and",
    "and_eq",
    "asm",
    "atomic_cancel",
    "atomic_commit",
    "atomic_noexcept",
    "auto",
    "bitand",
    "bitor",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "char8_t",
    "char16_t",
    "char32_t",
    "class",
    "compl",
    "concept",
    "const",
    "consteval",
    "constexpr",
    "constinit",
    "const_cast",
    "continue",
    "co_await",
    "co_return",
    "co_yield",
    "decltype",
    "default",
    "delete",
    "do",
    "double",
    "dynamic_cast",
    "else",
    "enum",
    "explicit",
    "export",
    "extern",
    "false",
    "float",
    "for",
    "friend",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "not",
    "not_eq",
    "nullptr",
    "operator",
    "or",
    "or_eq",
    "private",
    "protected",
    "public",
    "reflexpr",
    "register",
    "reinterpret_cast",
    "requires",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "static_assert",
    "static_cast",
    "struct",
    "switch",
    "synchronized",
    "template",
    "this",
    "thread_local",
    "throw",
    "true",
    "try",
    "typedef",
    "typeid",
    "typename",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "volatile",
    "wchar_t",
    "while",
    "xor",
    "xor_eq",
    // C-only keywords.
    "restrict",
    "_Alignas",
    "_Alignof",
    "_Atomic",
    "_Bool",
    "_Complex",
    "_Generic",
    "_Imaginary",
    "_Noreturn",
    "_Static_assert",
    "_Thread_local",
    // Macros and names the generated code relies on.
    "status",
    "err",
    "self",
    "ctx",
    "NULL",
    "EOF",
    "errno",
    "assert",
    "major",
    "minor",
    "uniffi",
];

/// Appends `_` to names that are keywords or clash with the generated code's own names.
pub(crate) fn escape(name: &str) -> String {
    if KEYWORDS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_owned()
    }
}

/// A function, method, argument or field name, kept as written in Rust (or UDL).
pub(crate) fn var(name: &str) -> String {
    escape(name)
}

/// Field name inside a variant: nameless fields become `v0`, `v1`, ...
pub(crate) fn field(name: &str, index: usize) -> String {
    if name.is_empty() {
        format!("v{index}")
    } else {
        var(name)
    }
}

/// Substitute `@key@` tokens in `template`.
pub(crate) fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = template.to_owned();
    for (key, value) in values {
        out = out.replace(&format!("@{key}@"), value);
    }
    out
}

/// Quote `s` as a C string literal.
pub(crate) fn string_literal(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}\"\"", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A doc comment, indented by `indent` spaces.
pub(crate) fn doc(docstring: Option<&str>, indent: usize) -> String {
    let Some(text) = docstring else {
        return String::new();
    };
    let pad = " ".repeat(indent);
    let mut out = format!("{pad}/**\n");
    for line in text.trim_end().lines() {
        let line = line
            .strip_prefix(' ')
            .unwrap_or(line)
            .trim_end()
            .replace("*/", "* /");
        out.push_str(&format!("{pad} * {line}\n").replace(" * \n", " *\n"));
    }
    out.push_str(&format!("{pad} */\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn var_keyword_gets_trailing_underscore() {
        assert_eq!(var("new"), "new_");
        assert_eq!(var("restrict"), "restrict_");
        assert_eq!(var("status"), "status_");
        assert_eq!(var("connect"), "connect");
    }

    #[test]
    fn field_without_name_uses_its_index() {
        assert_eq!(field("", 0), "v0");
        assert_eq!(field("client_id", 3), "client_id");
    }

    #[test]
    fn string_literal_escapes_quotes_backslashes_and_newlines() {
        assert_eq!(string_literal("a\"b\\c\n"), r#""a\"b\\c\n""#);
    }

    #[test]
    fn doc_comment_cannot_end_the_comment_early() {
        assert_eq!(doc(Some("a */ b"), 0), "/**\n * a * / b\n */\n");
    }
}
