use anyhow::{anyhow, Error};
use clap::Parser;

use mitra_config::Config;
use mitra_services::{
    monero::{
        wallet::{
            create_monero_signature,
            create_monero_wallet,
            get_active_addresses,
            open_monero_wallet,
            verify_monero_signature,
        },
    },
};

/// Create Monero wallet
/// (can be used when monero-wallet-rpc runs with --wallet-dir option)
#[derive(Parser)]
pub struct CreateMoneroWallet {
    name: String,
    password: Option<String>,
}

impl CreateMoneroWallet {
    pub async fn execute(
        self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        create_monero_wallet(
            monero_config,
            self.name,
            self.password,
        ).await?;
        println!("wallet created");
        Ok(())
    }
}

/// Create Monero signature
#[derive(Parser)]
pub struct CreateMoneroSignature {
    message: String,
}

impl CreateMoneroSignature {
    pub async fn execute(
        self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        let (address, signature) =
            create_monero_signature(monero_config, &self.message).await?;
        println!("address: {}", address);
        println!("signature: {}", signature);
        Ok(())
    }
}

/// Verify Monero signature
#[derive(Parser)]
pub struct VerifyMoneroSignature {
    address: String,
    message: String,
    signature: String,
}

impl VerifyMoneroSignature {
    pub async fn execute(
        self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        verify_monero_signature(
            monero_config,
            &self.address,
            &self.message,
            &self.signature,
        ).await?;
        println!("signature verified");
        Ok(())
    }
}

#[derive(Parser)]
pub struct ListActiveAddresses;

impl ListActiveAddresses {
    pub async fn execute(
        self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        let wallet_client = open_monero_wallet(monero_config).await?;
        let addresses = get_active_addresses(
            &wallet_client,
            monero_config.account_index,
        ).await?;
        for (address, amount) in addresses {
            println!("{}: {}", address, amount);
        };
        Ok(())
    }
}
