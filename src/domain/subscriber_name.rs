use unicode_segmentation::UnicodeSegmentation;

// SubscriberName struct here is of unname field of String type
// It is impossible to create an instance of SubscriberName without using the parse function
#[derive(Debug)]
pub struct SubscriberName(String);

impl SubscriberName {
    pub fn parse(s: String) -> Result<SubscriberName, String> {
        // A name is valid if it is not empty and does not consist solely of whitespace
        let is_empty_or_whitespace = s.trim().is_empty();

        // A name is valid if its length is less than or equal to 256 graphemes
        // (a grapheme is a user-perceived character, which may consist of multiple Unicode code points. For example, "é" can be represented as "e" + acute accent)
        let is_too_long = s.graphemes(true).count() > 256;

        // A name is valid if it does not contain any of the following forbidden characters: / ( ) " < > \ { }
        let forbidden_characters = ['/', '(', ')', '"', '<', '>', '\\', '{', '}'];
        let contains_forbidden_characters = s.chars().any(|c| forbidden_characters.contains(&c));

        // The name is valid if it is not empty/whitespace, not too long, and does not contain forbidden characters
        if is_empty_or_whitespace || is_too_long || contains_forbidden_characters {
            Err(format!("{} is not a valid subscriber name.", s))
        } else {
            Ok(Self(s))
        }
    }
}

impl AsRef<str> for SubscriberName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Unit tests for the domain module
#[cfg(test)]
mod tests {
    use crate::domain::SubscriberName;
    use claim::{assert_err, assert_ok};

    #[test]
    fn a_256_grapheme_name_is_valid() {
        let name = "a".repeat(256);
        assert_ok!(SubscriberName::parse(name));
    }

    #[test]
    fn a_name_longer_than_256_graphemes_is_invalid() {
        let name = "a".repeat(257);
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn whitespace_only_names_are_rejected() {
        let name = " ".to_string();
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn empty_string_is_rejected() {
        let name = "".to_string();
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn names_containing_forbidden_characters_are_rejected() {
        for name in &['/', '(', ')', '"', '<', '>', '\\', '{', '}'] {
            let name = name.to_string();
            assert_err!(SubscriberName::parse(name));
        }
    }

    #[test]
    fn valid_name_is_parsed_successfully() {
        let name = "John Doe".to_string();
        assert_ok!(SubscriberName::parse(name));
    }
}
