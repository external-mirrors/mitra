use std::str::FromStr;

use apx_core::caip2::MoneroNetwork;
use monero::{
    network::Network,
    util::{
        address::{
            Error as AddressError,
            PaymentId,
        },
        key::{Error as KeyError},
    },
};

pub use monero::util::{
    address::{Address, AddressType},
    key::PrivateKey,
};

pub const LOCK_DURATION: u64 = 10; // blocks
pub const BLOCK_TIME: u16 = 120;

pub fn address_network(address: Address) -> MoneroNetwork {
    match address.network {
        Network::Mainnet => MoneroNetwork::Mainnet,
        Network::Stagenet => MoneroNetwork::Stagenet,
        Network::Testnet => MoneroNetwork::Testnet,
    }
}

pub fn parse_monero_address(address: &str) -> Result<Address, AddressError> {
    Address::from_str(address)
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
