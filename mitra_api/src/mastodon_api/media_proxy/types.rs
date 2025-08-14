use serde::Deserialize;

#[derive(Deserialize)]
pub struct MediaProxyParams {
    #[serde(with = "hex")]
    pub signature: Vec<u8>,
}
