#[derive(Debug, PartialEq)]
pub struct Origin(String, String, u16);

impl Origin {
    pub fn new(scheme: &str, host: &str, port: u16) -> Self {
        Self(scheme.to_owned(), host.to_owned(), port)
    }
}
