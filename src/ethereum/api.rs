use web3::{
    api::Web3,
    contract::Contract,
    transports::Http,
};

pub type EthereumApi = Web3<Http>;
pub type EthereumContract = Contract<Http>;

pub fn connect(json_rpc_url: &str) -> Result<Web3<Http>, web3::Error> {
    let transport = Http::new(json_rpc_url)?;
    let connection = Web3::new(transport);
    Ok(connection)
}
