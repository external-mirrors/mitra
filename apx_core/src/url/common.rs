use iri_string::percent_encode::PercentEncodedForUri;
use percent_encoding::percent_decode_str;

#[derive(Debug, PartialEq)]
pub struct Origin(String, String, u16);

impl Origin {
    pub fn new(scheme: &str, host: &str, port: u16) -> Self {
        Self(scheme.to_owned(), host.to_owned(), port)
    }
}

/// Encode URI path component (RFC-3986).
/// https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/encodeURIComponent#encoding_for_rfc3986
pub fn url_encode(input: &str) -> String {
    PercentEncodedForUri::unreserve(input).to_string()
}

pub fn url_decode(input: &str) -> String {
    let bytes = percent_decode_str(input);
    bytes.decode_utf8_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encode_decode() {
        let input = "El Niño"; // unicode and space
        let output = url_encode(input);
        assert_eq!(output, "El%20Ni%C3%B1o");
        let decoded = url_decode(&output);
        assert_eq!(decoded, input);
    }

    #[test]
    fn test_url_encode_decode_reserved_characters() {
        let input_1 = ";/?:@&=+$,#"; // encoded by encodeURIComponent()
        let encoded_1 = url_encode(input_1);
        assert_eq!(encoded_1, "%3B%2F%3F%3A%40%26%3D%2B%24%2C%23");
        let decoded_1 = url_decode(&encoded_1);
        assert_eq!(decoded_1, input_1);

        let input_2 = "-.!~*'()"; // not encoded by encodeURIComponent()
        let encoded_2 = url_encode(input_2);
        assert_eq!(encoded_2, "-.%21~%2A%27%28%29");
        let decoded_2 = url_decode(&encoded_2);
        assert_eq!(decoded_2, input_2);
    }

    #[test]
    fn test_url_encode_decode_url() {
        let input = "https://social.example/users/test_user";
        let output = url_encode(input);
        assert_eq!(output, "https%3A%2F%2Fsocial.example%2Fusers%2Ftest_user");
        let decoded = url_decode(&output);
        assert_eq!(decoded, input);
    }
}
