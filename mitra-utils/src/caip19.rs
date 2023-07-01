/// https://chainagnostic.org/CAIPs/caip-19
use std::fmt;

use super::caip2::ChainId;

pub struct AssetType {
    chain_id: ChainId,
    namespace: String,
    reference: String,
}

// https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-20.md
const SLIP_44: &str = "slip44";
const SLIP_44_MONERO: u16 = 128;

impl AssetType {
    pub fn monero(chain_id: &ChainId) -> Self {
        assert!(chain_id.is_monero());
        Self {
            chain_id: chain_id.clone(),
            namespace: SLIP_44.to_string(),
            reference: SLIP_44_MONERO.to_string(),
        }
    }

    pub fn into_uri(self) -> String {
        // 'caip:' URI scheme has not been standardized
        // https://github.com/ChainAgnostic/CAIPs/issues/67
        format!("caip:19:{}", self)
    }
}

impl fmt::Display for AssetType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}/{}:{}",
            self.chain_id,
            self.namespace,
            self.reference,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monero() {
        let monero_mainnet = ChainId::monero_mainnet();
        let monero = AssetType::monero(&monero_mainnet);
        assert_eq!(
            monero.to_string(),
            "monero:418015bb9ae982a1975da7d79277c270/slip44:128",
        );
        assert_eq!(
            monero.into_uri(),
            "caip:19:monero:418015bb9ae982a1975da7d79277c270/slip44:128",
        );
    }
}
