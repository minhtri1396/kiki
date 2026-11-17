//! Small text helpers shared by the generate and model code.

/// Trims `text` and folds every run of white space, including line breaks, into one space.
pub(crate) fn one_line(text: &str) -> String {
    let text = text.trim().replace("\r\n", " ").replace(['\n', '\r'], " ");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `message[..n]`, backed off to a character boundary.
pub(crate) fn truncate_bytes(message: &str, n: usize) -> String {
    if message.len() <= n {
        return message.to_string();
    }
    let mut end = n;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_string()
}
