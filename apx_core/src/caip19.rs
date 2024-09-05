/// https://chainagnostic.org/CAIPs/caip-19
use std::fmt;
use std::str::FromStr;

use regex::Regex;

use super::caip2::{ChainId, MoneroNetwork, CAIP2_RE};

const CAIP19_ASSET_RE: &str = r"(?P<asset_namespace>[-a-z0-9]{3,8}):(?P<asset_reference>[-.%a-zA-Z0-9]{1,128})";

// 'caip:' URI scheme has not been standardized
// https://github.com/ChainAgnostic/CAIPs/issues/67
const CAIP19_URI_PREFIX: &str = "caip:19:";

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

#[derive(Debug, PartialEq)]
pub struct AssetType {
    pub chain_id: ChainId,
    asset_namespace: String,
    asset_reference: String,
}

#[derive(thiserror::Error, Debug)]
#[error("invalid CAIP-19 asset type")]
pub struct AssetTypeError;

impl AssetType {
    pub fn new(
        chain_id: ChainId,
        asset_namespace: &str,
        asset_reference: &str,
    ) -> Result<Self, AssetTypeError> {
        if chain_id.is_monero() {
            let slip44_coin_type = chain_id_to_slip44(&chain_id)
                .ok_or(AssetTypeError)?;
            if asset_namespace != SLIP44 ||
                asset_reference != slip44_coin_type.to_string()
            {
                return Err(AssetTypeError);
            };
            let asset_type = Self {
                chain_id: chain_id,
                asset_namespace: asset_namespace.to_owned(),
                asset_reference: asset_reference.to_owned(),
            };
            Ok(asset_type)
        } else {
            Err(AssetTypeError)
        }
    }

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

    pub fn is_monero(&self) -> bool {
        self.chain_id.is_monero() &&
            self.asset_namespace == SLIP44 &&
            self.asset_reference == SLIP44_MONERO.to_string()
    }

    pub fn to_uri(&self) -> String {
        format!("{}{}", CAIP19_URI_PREFIX, self)
    }

    pub fn from_uri(uri: &str) -> Result<Self, AssetTypeError> {
        let asset_type_str = uri.strip_prefix(CAIP19_URI_PREFIX)
            .ok_or(AssetTypeError)?;
        let asset_type = asset_type_str.parse()?;
        Ok(asset_type)
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

impl FromStr for AssetType {
    type Err = AssetTypeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let caip19_re_str = format!("{}/{}", CAIP2_RE, CAIP19_ASSET_RE);
        let caip19_re = Regex::new(&caip19_re_str)
            .expect("regexp should be valid");
        let caps = caip19_re.captures(value).ok_or(AssetTypeError)?;
        let chain_id = ChainId::new(&caps["namespace"], &caps["reference"])
            .map_err(|_| AssetTypeError)?;
        Self::new(
            chain_id,
            &caps["asset_namespace"],
            &caps["asset_reference"],
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
    }

    #[test]
    fn test_parse_asset_type_monero() {
        let value = "monero:418015bb9ae982a1975da7d79277c270/slip44:128";
        let asset_type = value.parse::<AssetType>().unwrap();
        assert_eq!(asset_type.is_monero(), true);
    }

    #[test]
    fn test_parse_asset_type_ethereum() {
        let value = "eip155:1/erc20:0x8f8221afbb33998d8584a2b05749ba73c37a938a";
        let result = value.parse::<AssetType>();
        assert!(matches!(result, Err(AssetTypeError)));
    }

    #[test]
    fn test_asset_type_uri() {
        let monero_mainnet = ChainId::monero_mainnet();
        let monero = AssetType::monero(&monero_mainnet).unwrap();
        let asset_uri = monero.to_uri();
        assert_eq!(
            asset_uri,
            "caip:19:monero:418015bb9ae982a1975da7d79277c270/slip44:128",
        );
        let asset_type = AssetType::from_uri(&asset_uri).unwrap();
        assert_eq!(asset_type, monero);
    }
}
