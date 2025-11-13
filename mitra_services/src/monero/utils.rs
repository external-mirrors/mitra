use std::str::FromStr;

use monero_rpc::monero::{
    util::address::Error as AddressError,
    Address,
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
