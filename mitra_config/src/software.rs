#[derive(Clone, Copy, Default)]
pub struct SoftwareMetadata {
    pub name: &'static str,
    pub version: &'static str,
    pub repository: &'static str,
}
