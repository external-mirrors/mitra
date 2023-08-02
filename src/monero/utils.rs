use std::str::FromStr;

use monero_rpc::monero::{
    util::address::Error as AddressError,
    Address,
};

pub fn parse_monero_address(address: &str) -> Result<Address, AddressError> {
    Address::from_str(address)
}

pub fn validate_monero_address(address: &str) -> Result<(), AddressError> {
    parse_monero_address(address)?;
    Ok(())
}
