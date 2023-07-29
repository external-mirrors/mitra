#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub struct ValidationError(pub &'static str);
