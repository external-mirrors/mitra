/// https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-2.md
use std::fmt;
use std::str::FromStr;

use regex::Regex;
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
    de::Error as DeserializerError,
};

pub(super) const CAIP2_RE: &str = r"(?P<namespace>[-a-z0-9]{3,8}):(?P<reference>[-a-zA-Z0-9]{1,32})";

const CAIP2_ETHEREUM_NAMESPACE: &str = "eip155";
const CAIP2_MONERO_NAMESPACE: &str = "monero";

const ETHEREUM_MAINNET_ID: u64 = 1;
const ETHEREUM_DEVNET_ID: u64 = 31337;

fn parse_ethereum_chain_id(reference: &str) -> Result<u32, ChainIdError> {
    let chain_id: u32 = reference.parse()
        .map_err(|_| ChainIdError("invalid EIP-155 chain ID"))?;
    Ok(chain_id)
}

// https://github.com/ChainAgnostic/namespaces/blob/main/monero/caip2.md
const MONERO_CHAIN_ID_RE: &str = r"[0-9a-f]{32}";
const MONERO_MAINNET_ID: &str = "418015bb9ae982a1975da7d79277c270";
const MONERO_STAGENET_ID: &str = "76ee3cc98646292206cd3e86f74d88b4";
const MONERO_TESTNET_ID: &str = "48ca7cd3c8de5b6a4d53d2861fbdaedc";
const MONERO_PRIVATE_ID: &str = "00000000000000000000000000000000";
const WOWNERO_MAINNET_ID: &str = "a3fd635dd5cb55700317783469ba749b";

#[derive(Debug, PartialEq)]
pub enum MoneroNetwork {
    Mainnet,
    Stagenet,
    Testnet,
    Private,
}

impl FromStr for MoneroNetwork {
    type Err = ChainIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        // https://www.getmonero.org/resources/developer-guides/daemon-rpc.html#get_info
        // nettype: string, one of mainnet, stagenet or testnet.
        let network = match value {
            "mainnet" => Self::Mainnet,
            "stagenet" => Self::Stagenet,
            "testnet" => Self::Testnet,
            "fakechain" | "regtest" => Self::Private,
            _ => return Err(ChainIdError("invalid monero network name")),
        };
        Ok(network)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Namespace {
    Eip155,
    Monero,
}

impl FromStr for Namespace {
    type Err = ChainIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let namespace = match value {
            CAIP2_ETHEREUM_NAMESPACE => Self::Eip155,
            CAIP2_MONERO_NAMESPACE => Self::Monero,
            _ => return Err(ChainIdError("unsupported CAIP-2 namespace")),
        };
        Ok(namespace)
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Eip155 => CAIP2_ETHEREUM_NAMESPACE,
            Self::Monero => CAIP2_MONERO_NAMESPACE,
        };
        write!(formatter, "{value}")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChainId {
    namespace: Namespace,
    reference: String,
}

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub struct ChainIdError(&'static str);

impl ChainId {
    pub fn new(
        namespace: &str,
        reference: &str,
    ) -> Result<Self, ChainIdError> {
        let namespace = Namespace::from_str(namespace)?;
        let reference = match namespace {
            Namespace::Eip155 => {
                parse_ethereum_chain_id(reference)?; // validation
                reference
            },
            Namespace::Monero => {
                // Allow CAIP-2 IDs and network names,
                // but always use IDs internally.
                let maybe_network = MoneroNetwork::from_str(reference);
                match maybe_network {
                    Ok(network) => return Ok(Self::from_monero_network(network)),
                    Err(_) => {
                        // CAIP-2 chain ID?
                        let chain_id_re = Regex::new(MONERO_CHAIN_ID_RE)
                            .expect("regexp should be valid");
                        if !chain_id_re.is_match(reference) {
                            return Err(ChainIdError("invalid monero chain ID"));
                        };
                        reference
                    },
                }
            },
        };
        let chain_id = Self {
            namespace: namespace,
            reference: reference.to_string(),
        };
        Ok(chain_id)
    }

    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    pub fn from_ethereum_chain_id(chain_id: u64) -> Self {
        Self {
            namespace: Namespace::Eip155,
            reference: chain_id.to_string(),
        }
    }

    pub fn ethereum_mainnet() -> Self {
        Self::from_ethereum_chain_id(ETHEREUM_MAINNET_ID)
    }

    pub fn ethereum_devnet() -> Self {
        Self::from_ethereum_chain_id(ETHEREUM_DEVNET_ID)
    }

    pub fn is_ethereum(&self) -> bool {
        self.namespace == Namespace::Eip155
    }

    pub fn ethereum_chain_id(&self) -> Result<u32, ChainIdError> {
        if !self.is_ethereum() {
            return Err(ChainIdError("namespace is not eip155"));
        };
        parse_ethereum_chain_id(&self.reference)
    }

    pub fn from_monero_network(network: MoneroNetwork) -> Self {
        let reference = match network {
            MoneroNetwork::Mainnet => MONERO_MAINNET_ID,
            MoneroNetwork::Stagenet => MONERO_STAGENET_ID,
            MoneroNetwork::Testnet => MONERO_TESTNET_ID,
            MoneroNetwork::Private => MONERO_PRIVATE_ID,
        };
        Self {
            namespace: Namespace::Monero,
            reference: reference.to_string(),
        }
    }

    pub fn monero_mainnet() -> Self {
        Self::from_monero_network(MoneroNetwork::Mainnet)
    }

    pub fn is_monero(&self) -> bool {
        self.namespace == Namespace::Monero
    }

    pub(super) fn is_wownero_mainnet(&self) -> bool {
        self.is_monero() && self.reference == WOWNERO_MAINNET_ID
    }

    pub fn monero_network(&self) -> Result<MoneroNetwork, ChainIdError> {
        if !self.is_monero() {
            return Err(ChainIdError("namespace is not monero"));
        };
        let network = match self.reference.as_str() {
            MONERO_MAINNET_ID => MoneroNetwork::Mainnet,
            MONERO_STAGENET_ID => MoneroNetwork::Stagenet,
            MONERO_TESTNET_ID => MoneroNetwork::Testnet,
            MONERO_PRIVATE_ID => MoneroNetwork::Private,
            _ => return Err(ChainIdError("unknown monero network")),
        };
        Ok(network)
    }
}

impl FromStr for ChainId {
    type Err = ChainIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let caip2_re = Regex::new(CAIP2_RE)
            .expect("regexp should be valid");
        let caps = caip2_re.captures(value)
            .ok_or(ChainIdError("invalid chain ID"))?;
        let chain_id = Self::new(
            &caps["namespace"],
            &caps["reference"],
        )?;
        Ok(chain_id)
    }
}

impl fmt::Display for ChainId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.namespace, self.reference)
    }
}

impl Serialize for ChainId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ChainId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        String::deserialize(deserializer)?
            .parse().map_err(DeserializerError::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bitcoin_chain_id() {
        let value = "bip122:000000000019d6689c085ae165831e93";
        let error = value.parse::<ChainId>().err().unwrap();
        assert!(matches!(error, ChainIdError("unsupported CAIP-2 namespace")));
    }

    #[test]
    fn test_parse_ethereum_chain_id() {
        let value = "eip155:1";
        let chain_id = value.parse::<ChainId>().unwrap();
        assert_eq!(chain_id.namespace, Namespace::Eip155);
        assert_eq!(chain_id.reference, "1");
        assert_eq!(chain_id.to_string(), value);
    }

    #[test]
    fn test_parse_monero_chain_id() {
        let value = "monero:418015bb9ae982a1975da7d79277c270";
        let chain_id = value.parse::<ChainId>().unwrap();
        assert_eq!(chain_id.namespace, Namespace::Monero);
        assert_eq!(chain_id.reference, "418015bb9ae982a1975da7d79277c270");
        assert_eq!(chain_id.to_string(), value);
    }

    #[test]
    fn test_parse_monero_chain_id_alias() {
        let value = "monero:mainnet";
        let chain_id = value.parse::<ChainId>().unwrap();
        assert_eq!(chain_id.namespace, Namespace::Monero);
        assert_eq!(chain_id.reference, "418015bb9ae982a1975da7d79277c270");
    }

    #[test]
    fn test_parse_monero_chain_id_invalid() {
        let value = "monero:0x418015bb9ae982a1975da7d79277c270";
        let error = value.parse::<ChainId>().err().unwrap();
        assert!(matches!(error, ChainIdError("invalid monero chain ID")));
    }

    #[test]
    fn test_parse_invalid_chain_id() {
        let value = "eip155/1/abcde";
        let error = value.parse::<ChainId>().err().unwrap();
        assert!(matches!(error, ChainIdError("invalid chain ID")));
    }

    #[test]
    fn test_ethereum_chain_id() {
        let chain_id: ChainId = "eip155:1".parse().unwrap();
        let result = chain_id.ethereum_chain_id().unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_ethereum_chain_id_not_ethereum() {
        let chain_id: ChainId = "monero:mainnet".parse().unwrap();
        let error = chain_id.ethereum_chain_id().err().unwrap();
        assert!(matches!(error, ChainIdError("namespace is not eip155")));
    }

    #[test]
    fn test_monero_network() {
        let chain_id: ChainId = "monero:regtest".parse().unwrap();
        let network = chain_id.monero_network().unwrap();
        assert_eq!(network, MoneroNetwork::Private);
    }
}
