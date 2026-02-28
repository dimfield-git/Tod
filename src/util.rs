use std::fmt;

/// Emit a warning to stderr with a `warning: ` prefix.
pub fn warn(args: fmt::Arguments) {
    eprintln!("warning: {}", args);
}

/// Emit a warning to stderr. Wrapper for future structured logging.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::util::warn(format_args!($($arg)*))
    };
}

/// Truncate a string for error messages without panicking on UTF-8 boundaries.
pub fn safe_preview(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::safe_preview;

    #[test]
    fn safe_preview_within_limit() {
        let s = "hello";
        assert_eq!(safe_preview(s, 10), "hello");
    }

    #[test]
    fn safe_preview_truncates() {
        let s = "hello world";
        assert_eq!(safe_preview(s, 5), "hello");
    }

    #[test]
    fn safe_preview_multibyte() {
        let s = "a🙂b";
        assert_eq!(safe_preview(s, 2), "a");
    }
}
