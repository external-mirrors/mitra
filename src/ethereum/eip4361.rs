/// Sign-In with Ethereum https://eips.ethereum.org/EIPS/eip-4361
use hex::FromHex;
use siwe::Message;
use web3::types::H160;

use mitra_utils::{
    caip10::AccountId,
    caip2::ChainId,
};

use crate::validators::errors::ValidationError;

use super::utils::address_to_string;

pub struct Eip4361SessionData {
    pub account_id: AccountId,
    pub nonce: String,
}

/// Verifies EIP-4361 signature and returns wallet address
pub fn verify_eip4361_signature(
    message: &str,
    signature: &str,
    instance_hostname: &str,
    login_message: &str,
) -> Result<Eip4361SessionData, ValidationError> {
    let message: Message = message.parse()
        .map_err(|_| ValidationError("invalid EIP-4361 message"))?;
    let signature_bytes = <[u8; 65]>::from_hex(signature.trim_start_matches("0x"))
        .map_err(|_| ValidationError("invalid signature string"))?;
    if message.domain != instance_hostname {
        return Err(ValidationError("domain doesn't match instance hostname"));
    };
    let statement = message.statement.as_ref()
        .ok_or(ValidationError("statement is missing"))?;
    if statement != login_message {
        return Err(ValidationError("statement doesn't match login message"));
    };
    if !message.valid_now() {
        return Err(ValidationError("message is not currently valid"));
    };
    message.verify_eip191(&signature_bytes)
        .map_err(|_| ValidationError("invalid signature"))?;

    let chain_id = ChainId::from_ethereum_chain_id(message.chain_id);
    // Return wallet address in lower case
    let wallet_address = address_to_string(H160(message.address));
    let account_id = AccountId { chain_id, address: wallet_address };
    let session_data = Eip4361SessionData {
        account_id,
        nonce: message.nonce,
    };
    Ok(session_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_HOSTNAME: &str = "example.com";
    const LOGIN_MESSAGE: &str = "test";

    #[test]
    fn test_verify_eip4361_signature() {
        let message = "example.com wants you to sign in with your Ethereum account:
0x70997970C51812dc3A010C7d01b50e0d17dc79C8

test

URI: https://example.com
Version: 1
Chain ID: 1
Nonce: 3cb7760eac2f
Issued At: 2022-02-14T22:27:35.500Z";
        let signature = "0x9059c9a69c31e87d887262a574abcc33f320d5b778bea8a35c6fbdea94a17e9652b99f7cdd146ed67fa8e4bb02462774b958a129c421fe8d743a43bf67dcbcd61c";
        let session_data = verify_eip4361_signature(
            message, signature,
            INSTANCE_HOSTNAME,
            LOGIN_MESSAGE,
        ).unwrap();
        assert_eq!(session_data.account_id.chain_id, ChainId::ethereum_mainnet());
        assert_eq!(session_data.account_id.address, "0x70997970c51812dc3a010c7d01b50e0d17dc79c8");
        assert_eq!(session_data.nonce, "3cb7760eac2f");
    }

    #[test]
    fn test_verify_eip4361_signature_invalid() {
        let message = "abc";
        let signature = "xyz";
        let error = verify_eip4361_signature(
            message, signature,
            INSTANCE_HOSTNAME,
            LOGIN_MESSAGE,
        ).err().unwrap();
        assert_eq!(error.to_string(), "invalid EIP-4361 message");
    }
}
