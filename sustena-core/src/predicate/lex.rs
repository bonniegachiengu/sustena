//! Tokenizer for the predicate language.
//!
//! Character-by-character, matching the reference implementation exactly —
//! including its quirks, because parity is measured on behaviour:
//!
//!   - a dot binds INTO an identifier, so `finances.liquid.balance` is one
//!     token, not three;
//!   - only `=` forms a two-character operator (`>=`, `<=`, `==`, `!=`), so a
//!     stray `!` or `=` is a single token;
//!   - an unrecognised character is skipped rather than rejected. That is
//!     deliberate in the reference: a trailing `.` left over after `]` would
//!     otherwise break an otherwise-valid expression.

pub const COMPARISON_OPS: [&str; 6] = [">=", "<=", "==", "!=", ">", "<"];
pub const AGGREGATE_FUNCS: [&str; 5] = ["SUM", "COUNT", "AVG", "MIN", "MAX"];
pub const KEYWORDS: [&str; 11] = [
    "AND", "OR", "NOT", "IN", "ALL", "EXISTS", "SUM", "COUNT", "AVG", "MIN", "MAX",
];

pub fn is_keyword(token: &str) -> bool {
    let upper = token.to_ascii_uppercase();
    KEYWORDS.contains(&upper.as_str())
}

pub fn tokenize(expr: &str) -> Vec<String> {
    let chars: Vec<char> = expr.trim().chars().collect();
    let n = chars.len();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < n {
        let c = chars[i];

        if c.is_whitespace() || c == ',' {
            i += 1;
            continue;
        }

        // Quoted string — scan to the matching quote, honouring backslash escapes.
        if c == '"' || c == '\'' {
            let quote = c;
            let mut j = i + 1;
            while j < n {
                if chars[j] == '\\' {
                    j += 2;
                    continue;
                }
                if chars[j] == quote {
                    break;
                }
                j += 1;
            }
            let end = (j + 1).min(n);
            tokens.push(chars[i..end].iter().collect());
            i = end;
            continue;
        }

        if matches!(c, '[' | ']' | '(' | ')' | ':' | '*') {
            tokens.push(c.to_string());
            i += 1;
            continue;
        }

        if matches!(c, '>' | '<' | '!' | '=') {
            if i + 1 < n && chars[i + 1] == '=' {
                tokens.push(chars[i..i + 2].iter().collect());
                i += 2;
            } else {
                tokens.push(c.to_string());
                i += 1;
            }
            continue;
        }

        if c.is_ascii_digit() {
            let start = i;
            while i < n && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
            continue;
        }

        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '.' || chars[i] == '_') {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
            continue;
        }

        // Unrecognised — skip, as the reference does.
        i += 1;
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_paths_are_one_token() {
        assert_eq!(tokenize("finances.liquid.balance >= 0"),
                   vec!["finances.liquid.balance", ">=", "0"]);
    }

    #[test]
    fn wildcard_and_brackets() {
        assert_eq!(tokenize("ALL finances.pockets[*].allocated >= 0"),
                   vec!["ALL", "finances.pockets", "[", "*", "]", "allocated", ">=", "0"]);
    }

    #[test]
    fn quoted_strings_survive_spaces() {
        assert_eq!(tokenize("status == \"in progress\""),
                   vec!["status", "==", "\"in progress\""]);
    }

    #[test]
    fn aggregate_call() {
        assert_eq!(tokenize("SUM(lines[*].debit)"),
                   vec!["SUM", "(", "lines", "[", "*", "]", "debit", ")"]);
    }
}
