//! Quote-aware command parser. Produces three views of a command string,
//! stripping different layers of quoting for downstream security validators.

/// Three parallel extractions of a single command string.
/// Maps to `bashSecurity.ts`: outside_single_quotes ≈ withDoubleQuotes,
/// fully_unquoted ≈ fullyUnquoted, unquoted_keep_quotes ≈ unquotedKeepQuoteChars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteExtraction {
    /// Content inside single quotes kept (delimiters stripped). DQ preserved.
    pub outside_single_quotes: String,
    /// Characters outside all quoted regions. Both delimiters and content stripped.
    pub fully_unquoted: String,
    /// Like `fully_unquoted` but keeps quote delimiters for adjacency checks.
    pub unquoted_keep_quotes: String,
}

/// Extract three quote-stripped views. Backslash escapes recognised outside
/// single quotes. Ported from Claude Code's `extractQuotedContent`.
pub fn extract_quotes(command: &str) -> QuoteExtraction {
    let mut outside_single = String::with_capacity(command.len());
    let mut fully_unquoted = String::with_capacity(command.len());
    let mut keep_quotes = String::with_capacity(command.len());
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    for c in command.chars() {
        // Previous char was a backslash -- consume escaped char literally.
        if escaped {
            escaped = false;
            if !in_single {
                outside_single.push(c);
            }
            // Escaped char is literal in all fields (backslash consumed).
            fully_unquoted.push(c);
            keep_quotes.push(c);
            continue;
        }

        // Backslash outside single quotes starts an escape sequence.
        // The backslash itself is consumed (not output).
        if c == '\\' && !in_single {
            escaped = true;
            continue;
        }

        // Toggle single-quote mode. Delimiter stripped from outside_single
        // and fully_unquoted; preserved in keep_quotes.
        if c == '\'' && !in_double {
            in_single = !in_single;
            keep_quotes.push(c);
            continue;
        }

        // Toggle double-quote mode. Delimiter stripped from outside_single
        // and fully_unquoted; preserved in keep_quotes. Content inside DQ
        // is literal and appears in all three fields.
        if c == '"' && !in_single {
            in_double = !in_double;
            keep_quotes.push(c);
            continue;
        }

        // Plain character: all content is literal in bash (SQ and DQ
        // both make their contents literal). The three fields differ
        // only in which delimiter characters they preserve:
        //   - outside_single: strips SQ delimiters
        //   - fully_unquoted: strips both SQ and DQ delimiters
        //   - keep_quotes:    preserves all delimiters
        outside_single.push(c);
        fully_unquoted.push(c);
        keep_quotes.push(c);
    }

    QuoteExtraction {
        outside_single_quotes: outside_single,
        fully_unquoted,
        unquoted_keep_quotes: keep_quotes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_trace() {
        let q = extract_quotes("echo 'hello world'");
        eprintln!("DEBUG outside_single: {:?}", q.outside_single_quotes);
        eprintln!("DEBUG fully_unquoted: {:?}", q.fully_unquoted);
        eprintln!("DEBUG keep_quotes: {:?}", q.unquoted_keep_quotes);
    }

    #[test]
    fn plain_command_unchanged() {
        let q = extract_quotes("ls -la /tmp");
        assert_eq!(q.outside_single_quotes, "ls -la /tmp");
        assert_eq!(q.fully_unquoted, "ls -la /tmp");
        assert_eq!(q.unquoted_keep_quotes, "ls -la /tmp");
    }

    #[test]
    fn single_quotes_delimiters_stripped_content_kept() {
        let q = extract_quotes("echo 'hello world'");
        assert_eq!(q.outside_single_quotes, "echo hello world");
        assert_eq!(q.fully_unquoted, "echo hello world");
        assert_eq!(q.unquoted_keep_quotes, "echo 'hello world'");
    }

    #[test]
    fn double_quoted_delimiters_stripped_in_fully_unquoted() {
        let q = extract_quotes("echo \"hello#world\"");
        assert_eq!(q.outside_single_quotes, "echo hello#world");
        assert_eq!(q.fully_unquoted, "echo hello#world");
        assert_eq!(q.unquoted_keep_quotes, "echo \"hello#world\"");
    }

    #[test]
    fn mixed_quotes() {
        let q = extract_quotes("rm 'foo' \"bar\"");
        assert_eq!(q.outside_single_quotes, "rm foo bar");
        assert_eq!(q.fully_unquoted, "rm foo bar");
        assert_eq!(q.unquoted_keep_quotes, "rm 'foo' \"bar\"");
    }

    #[test]
    fn backslash_escape_outside_quotes() {
        let q = extract_quotes(r"echo hello\ world");
        assert_eq!(q.outside_single_quotes, "echo hello world");
        assert_eq!(q.fully_unquoted, "echo hello world");
    }

    #[test]
    fn backslash_literal_inside_single_quotes() {
        let q = extract_quotes("echo 'hello\\ world'");
        assert_eq!(q.outside_single_quotes, "echo hello\\ world");
        assert_eq!(q.fully_unquoted, "echo hello\\ world");
    }

    #[test]
    fn backslash_double_quote_escape() {
        let q = extract_quotes(r#"echo "he\"llo""#);
        assert_eq!(q.outside_single_quotes, r#"echo he"llo"#);
        assert_eq!(q.fully_unquoted, r#"echo he"llo"#);
    }

    #[test]
    fn keep_quotes_preserves_single_delimiters() {
        let q = extract_quotes("echo 'x'#");
        assert_eq!(q.fully_unquoted, "echo x#");
        assert_eq!(q.unquoted_keep_quotes, "echo 'x'#");
    }

    #[test]
    fn keep_quotes_preserves_double_delimiters() {
        let q = extract_quotes("echo \"x\"#");
        assert_eq!(q.fully_unquoted, "echo x#");
        assert_eq!(q.unquoted_keep_quotes, "echo \"x\"#");
    }

    #[test]
    fn empty_command() {
        let q = extract_quotes("");
        assert_eq!(q.outside_single_quotes, "");
        assert_eq!(q.fully_unquoted, "");
    }

    #[test]
    fn unclosed_single_quote() {
        let q = extract_quotes("echo 'hello");
        assert_eq!(q.outside_single_quotes, "echo hello");
        assert_eq!(q.fully_unquoted, "echo hello");
    }

    #[test]
    fn unclosed_double_quote() {
        let q = extract_quotes("echo \"hello");
        assert_eq!(q.outside_single_quotes, "echo hello");
        assert_eq!(q.fully_unquoted, "echo hello");
    }

    #[test]
    fn backslash_at_end_of_command() {
        let q = extract_quotes("echo \\");
        assert_eq!(q.outside_single_quotes, "echo ");
    }

    #[test]
    fn single_inside_double_is_literal() {
        let q = extract_quotes("echo \"it's fine\"");
        assert_eq!(q.outside_single_quotes, "echo it's fine");
        assert_eq!(q.fully_unquoted, "echo it's fine");
    }

    #[test]
    fn double_inside_single_is_literal() {
        let q = extract_quotes("echo 'say \"hi\"'");
        assert_eq!(q.outside_single_quotes, "echo say \"hi\"");
        assert_eq!(q.fully_unquoted, "echo say \"hi\"");
    }

    #[test]
    fn consecutive_empty_single_quotes() {
        let q = extract_quotes("echo ''test''");
        assert_eq!(q.outside_single_quotes, "echo test");
        assert_eq!(q.fully_unquoted, "echo test");
        assert_eq!(q.unquoted_keep_quotes, "echo ''test''");
    }

    #[test]
    fn flag_obfuscation_pattern() {
        let q = extract_quotes(r#"find . "-exec" rm {} \;"#);
        assert_eq!(q.outside_single_quotes, "find . -exec rm {} ;");
        assert_eq!(q.fully_unquoted, "find . -exec rm {} ;");
    }
}

#[cfg(test)]
#[path = "quote_parser_tests.rs"]
mod quote_parser_tests;
