use std::path::Path;

use sysinfo::Disks;

pub fn get_available_disk_space(path: &Path) -> Result<usize, &'static str> {
    let absolute_path = path.canonicalize().map_err(|_| "invalid path")?;
    let disks = Disks::new_with_refreshed_list();
    let disk = disks.iter()
        .find(|disk| absolute_path.starts_with(disk.mount_point()))
        .ok_or("mountpoint is not found")?;
    let available = disk.available_space().try_into()
        .map_err(|_| "invalid number")?;
    Ok(available)
}
