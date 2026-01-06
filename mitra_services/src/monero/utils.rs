use std::str::FromStr;

use apx_core::caip2::{ChainId, MoneroNetwork};
use monero::{
    network::Network,
    util::{
        address::{
            AddressType,
            Error as AddressError,
            PaymentId,
        },
        key::{Error as KeyError},
    },
};

pub use monero::util::{
    address::Address,
    key::PrivateKey,
};

pub const LOCK_DURATION: u64 = 10; // blocks
pub const BLOCK_TIME: u16 = 120;

pub(super) fn address_network(address: Address) -> MoneroNetwork {
    match address.network {
        Network::Mainnet => MoneroNetwork::Mainnet,
        Network::Stagenet => MoneroNetwork::Stagenet,
        Network::Testnet => MoneroNetwork::Testnet,
    }
}

pub fn parse_monero_address(address: &str) -> Result<Address, AddressError> {
    Address::from_str(address)
}

fn is_valid_network(
    network: MoneroNetwork,
    expected_chain_id: &ChainId,
) -> bool {
    match expected_chain_id.monero_network() {
        Err(_) => false, // not Monero
        Ok(MoneroNetwork::Private) => true, // always valid
        Ok(expected_network) => network == expected_network,
    }
}

pub fn validate_monero_address(
    address: &str,
    expected_chain_id: &ChainId,
) -> Result<(), AddressError> {
    let address = parse_monero_address(address)?;
    let network = address_network(address);
    if !is_valid_network(network, expected_chain_id) {
        return Err(AddressError::InvalidFormat);
    };
    Ok(())
}

pub fn validate_monero_standard_address(
    address: &str,
    expected_chain_id: &ChainId,
) -> Result<(), AddressError> {
    let address = parse_monero_address(address)?;
    if address.addr_type != AddressType::Standard {
        return Err(AddressError::InvalidFormat);
    };
    let network = address_network(address);
    if !is_valid_network(network, expected_chain_id) {
        return Err(AddressError::InvalidFormat);
    };
    Ok(())
}

pub fn parse_monero_view_key(view_key: &str) -> Result<PrivateKey, KeyError> {
    PrivateKey::from_str(view_key)
}

pub fn create_integrated_address(
    primary_address: Address,
) -> Address {
    let payment_id = PaymentId::random();
    Address::integrated(
        primary_address.network,
        primary_address.public_spend,
        primary_address.public_view,
        payment_id,
    )
}

pub fn get_payment_id(address: Address) -> Option<PaymentId> {
    if let AddressType::Integrated(payment_id) = address.addr_type {
        Some(payment_id)
    } else {
        None
    }
}
