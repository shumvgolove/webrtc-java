//
// Copyright 2019-2021 Signal Messenger, LLC
// SPDX-License-Identifier: AGPL-3.0-only
//

/// Truncate the given string |s| by retaining only a brief prefix and suffix, to match
/// Desktop's truncateForLogging format.
/// If the string contains unicode, only output one character.
pub fn truncate_for_logging(s: &str) -> String {
    if cfg!(debug_assertions) && !cfg!(test) {
        // For debug testing/local builds only, allow the full string.
        s.to_string()
    } else {
        // Take a small number of characters, but fewer if they are non-ascii unicode, as
        // unicode provides a substantially higher amount of information per char.
        // (e.g. four mandarin characters could be a full name)
        let out: String = if s.is_ascii() {
            let n = s.chars().by_ref().count();
            if n <= 4 {
                return s.to_string();
            }
            let mut chars = s.chars();
            let mut out: String = chars.by_ref().take(2).collect();
            out.push_str("...");
            // n - 4 because we're reusing the iterator we took 2 from
            out.extend(chars.skip(n - 4));
            out
        } else {
            if s.chars().count() <= 1 {
                return s.to_string();
            }
            s.chars().take(1).chain("...".chars()).collect()
        };
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_for_logging() {
        assert_eq!(truncate_for_logging("0123456789"), "01...89");
        assert_eq!(truncate_for_logging("0123"), "0123");
        assert_eq!(truncate_for_logging("0"), "0");
        assert_eq!(truncate_for_logging("你好"), "你..."); // ni hao (hello)
        assert_eq!(truncate_for_logging("你"), "你");
        // This is not necessarily behavior we want to enforce, but the test is here for
        // documentation of the limitations of this implementation:
        // the string y̆, which looks like one character to humans, is represented as
        // two Unicode Scalar Values: y and \u{0306}.
        // Rust's standard library does not provide functionality to iterate by "grapheme clusters."
        //
        // While we could get such a library from crates.io, it would require us to build in a
        // (large) unicode table to the compiled library.
        assert_eq!(truncate_for_logging("y̆ is from rust str docs"), "y...");
    }
}
