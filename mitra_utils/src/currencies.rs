use crate::caip2::{ChainId, Namespace};

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

impl From<ChainId> for Currency {
    fn from(value: ChainId) -> Self {
        match value.namespace() {
            Namespace::Eip155 => Self::Ethereum,
            Namespace::Monero => Self::Monero,
        }
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

    #[test]
    fn test_chain_id_conversion() {
        let currency = Currency::from(ChainId::ethereum_mainnet());
        assert_eq!(currency, Currency::Ethereum);

        let currency = Currency::from(ChainId::monero_mainnet());
        assert_eq!(currency, Currency::Monero);
    }
}
