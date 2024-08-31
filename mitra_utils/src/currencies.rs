#[derive(Debug, PartialEq)]
pub enum Currency {
    Ethereum,
    Monero,
}

impl Currency {
    fn code(&self) -> String {
        match self {
            Self::Ethereum => "ETH",
            Self::Monero => "XMR",
        }.to_string()
    }

    pub fn field_name(&self) -> String {
        format!("${}", self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_currency_field_name() {
        let ethereum = Currency::Ethereum;
        assert_eq!(ethereum.field_name(), "$ETH");
    }
}
