use eth_blockies::{EthBlockies, BlockiesGenerator};

pub fn generate_identicon(input: &str) -> Vec<u8> {
    EthBlockies::png_data(input, (128, 128))
}

pub fn generate_pixel() -> Vec<u8> {
    EthBlockies::png_data([0], (1, 1))
}
