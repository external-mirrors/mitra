use std::str::FromStr;

use monero::{
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

pub fn parse_monero_address(address: &str) -> Result<Address, AddressError> {
    Address::from_str(address)
}

pub fn validate_monero_address(address: &str) -> Result<(), AddressError> {
    parse_monero_address(address)?;
    Ok(())
}

pub fn validate_monero_standard_address(address: &str) -> Result<(), AddressError> {
    let address = parse_monero_address(address)?;
    if address.addr_type != AddressType::Standard {
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
