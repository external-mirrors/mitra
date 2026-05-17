use rand::{Rng as _};

pub fn generate_random_sequence<const LEN: usize>() -> [u8; LEN] {
    let mut rng = rand::rng();
    let mut value = [0u8; LEN];
    rng.fill(&mut value[..]);
    value
}
