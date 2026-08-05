/// Escapes a string for use as a single-quoted Pinot SQL string literal
/// (doubling embedded single quotes, per standard SQL).
pub fn escape_string_literal(value: &str) -> String {
    value.replace('\'', "''")
}

/// Quotes an identifier (column/table name) for Pinot SQL, doubling any
/// embedded double quotes.
pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_single_quotes() {
        assert_eq!(escape_string_literal("O'Brien"), "O''Brien");
        assert_eq!(escape_string_literal("plain"), "plain");
    }

    #[test]
    fn quotes_identifiers() {
        assert_eq!(quote_identifier("column"), "\"column\"");
        assert_eq!(quote_identifier("weird\"col"), "\"weird\"\"col\"");
    }
}
