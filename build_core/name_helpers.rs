//! Name transformation helpers used to generate idiomatic Rust code.
//==================================================================================NAME_HELPERS
/// Convert `camelCase` or `PascalCase` into `snake_case`.
/// The suffix is appended when a keyword collision occurs (e.g. suffix "field"
/// turns `type` into `type_field`).
pub(crate) fn to_snake_case(field: &str, suffix: &'static str) -> String {
    let mut buffer = String::new();

    let temp_field = if RUST_KEYWORDS.contains(&field) {
        format!("{field}_{suffix}")
    } else {
        field.to_string()
    };

    temp_field.chars().for_each(|c| {
        if c.is_uppercase() && !buffer.is_empty() {
            buffer.push('_');
        }
        buffer.push(c.to_ascii_lowercase());
    });
    buffer
}

/// Aggressiveness levels applied when converting to `PascalCase`.
pub(crate) enum PascalCaseMode {
    /// Minimal transformations; keep the original granularity.
    Soft,
    /// Aggressive transformations suitable for enum/struct names.
    Hard,
}
/// Convert `snake_case` or `camelCase` into `PascalCase`.
/// The aggressiveness is controlled via `PascalCaseMode`. The `Hard` mode is
/// field/variant friendly.
pub(crate) fn to_pascal_case(name: &str, mode: PascalCaseMode) -> String {
    let mut buffer = String::new();
    let mut capitalize_next = true;

    let mut chars = name.chars().peekable();

    while let Some(c) = chars.next() {
        match mode {
            PascalCaseMode::Hard => match c {
                '-' => {
                    if let Some(next_char) = chars.peek() {
                        if next_char.is_ascii_digit() {
                            buffer.push_str("Remove");
                        }
                    }
                }
                '+' => {
                    if let Some(next_char) = chars.peek() {
                        if next_char.is_ascii_digit() {
                            buffer.push_str("Add");
                        }
                    }
                }

                '%' => {
                    capitalize_next = true;
                    buffer.push_str("Percent");
                }

                '<' => {
                    capitalize_next = true;
                    buffer.push_str("InfTo");
                }
                '>' => {
                    capitalize_next = true;
                    buffer.push_str("SupTo");
                }

                ' ' | '_' | '#' | '(' | ')' | '&' | '.' | ',' | '/' | '[' | ']' | '{' | '}' => {
                    capitalize_next = true;
                }
                _ if buffer.is_empty() && c.is_numeric() => {
                    buffer.push_str("Val");
                    buffer.push(c);
                    capitalize_next = true;
                }

                _ if capitalize_next => {
                    buffer.push(c.to_ascii_uppercase());
                    capitalize_next = false;
                }

                _ if c.is_numeric() => {
                    buffer.push(c);
                    capitalize_next = true;
                }

                _ if c.is_alphanumeric() => {
                    buffer.push(c);
                }

                _ => {}
            },

            PascalCaseMode::Soft => match c {
                _ if capitalize_next => {
                    buffer.push(c.to_ascii_uppercase());
                    capitalize_next = false;
                }

                _ if c.is_numeric() => {
                    buffer.push(c);
                    capitalize_next = true;
                }

                _ if c.is_alphanumeric() => {
                    buffer.push(c);
                }

                _ => {}
            },
        }
    }

    buffer
}

/// Reserved Rust keywords we must avoid when generating identifiers.
pub const RUST_KEYWORDS: &[&str] = &[
    // --- Strict Keywords ---
    "as",
    "break",
    "const",
    "continue",
    "crate",
    "else",
    "enum",
    "extern",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "pub",
    "ref",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "trait",
    "true",
    "type",
    "unsafe",
    "use",
    "where",
    "while",
    "async",
    "await",
    "dyn",
    // --- Reserved Keywords ---
    "abstract",
    "become",
    "box",
    "do",
    "final",
    "macro",
    "override",
    "priv",
    "typeof",
    "unsized",
    "virtual",
    "yield",
    "try",
    "gen",
    "union",
    "macro_rules",
    "raw",
    "safe",
    "keyword",
    "static",
];
