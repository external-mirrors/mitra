use std::path::Path;

use lfs_core::{read_mounts, ReadOptions};

pub fn get_available_disk_space(path: &Path) -> Result<u64, &'static str> {
    let absolute_path = path.canonicalize().map_err(|_| "invalid path")?;
    let options = ReadOptions::default();
    let mounts = read_mounts(&options).map_err(|_| "can't read mounts")?;
    let mount = mounts.iter()
        .find(|mount| absolute_path.starts_with(&mount.info.mount_point))
        .ok_or("mountpoint is not found")?;
    let stats = mount.stats().ok_or("can't get mount stats")?;
    Ok(stats.available())
}
