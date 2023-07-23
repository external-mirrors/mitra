/// https://chainagnostic.org/CAIPs/caip-19
use std::fmt;

use super::caip2::{ChainId, MoneroNetwork};

// https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-20.md
const SLIP44: &str = "slip44";
const SLIP44_TESTNET: u16 = 1;
const SLIP44_MONERO: u16 = 128;
const SLIP44_WOWNERO: u16 = 417;

fn chain_id_to_slip44(chain_id: &ChainId) -> Option<u16> {
    if chain_id.is_monero() {
        let coin_type = match chain_id.monero_network() {
            Ok(MoneroNetwork::Mainnet) => SLIP44_MONERO,
            Ok(_) => SLIP44_TESTNET,
            Err(_) => {
                if chain_id.is_wownero_mainnet() {
                    SLIP44_WOWNERO
                } else {
                    return None;
                }
            },
        };
        Some(coin_type)
    } else {
        None
    }
}

pub struct AssetType {
    chain_id: ChainId,
    asset_namespace: String,
    asset_reference: String,
}

#[derive(thiserror::Error, Debug)]
#[error("invalid CAIP-19 asset type")]
pub struct AssetTypeError;

impl AssetType {
    pub fn monero(chain_id: &ChainId) -> Result<Self, AssetTypeError> {
        if !chain_id.is_monero() {
            return Err(AssetTypeError);
        };
        let slip44_coin_type = chain_id_to_slip44(chain_id)
            .ok_or(AssetTypeError)?;
        let asset_type = Self {
            chain_id: chain_id.clone(),
            asset_namespace: SLIP44.to_string(),
            asset_reference: slip44_coin_type.to_string(),
        };
        Ok(asset_type)
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
            self.asset_namespace,
            self.asset_reference,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monero() {
        let monero_mainnet = ChainId::monero_mainnet();
        let monero = AssetType::monero(&monero_mainnet).unwrap();
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
