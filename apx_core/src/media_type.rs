use infer;

pub fn sniff_media_type(data: &[u8]) -> Option<String> {
    infer::get(data).map(|val| val.mime_type().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sniff_media_type() {
        let data = b"%PDF-1.5";
        let media_type = sniff_media_type(data).unwrap();
        assert_eq!(media_type, "application/pdf");
    }
}
