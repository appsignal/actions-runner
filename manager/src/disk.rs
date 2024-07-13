use camino::Utf8PathBuf;
use util::fs;

#[derive(Debug)]
pub struct Disk {
    pub size: u32,
    pub path: Utf8PathBuf,
    pub name: String,
    pub format: DiskFormat,
}

#[derive(Debug)]
pub enum DiskFormat {
    Ext4,
}

impl Disk {
    pub fn new(path: &Utf8PathBuf, name: &str, size: u32, format: DiskFormat) -> Self {
        Self {
            path: path.clone(),
            name: name.to_string(),
            size,
            format,
        }
    }

    pub fn size_in_megabytes(&self) -> u64 {
        self.size as u64 * 1024
    }

    pub fn size_in_kilobytes(&self) -> u64 {
        self.size_in_megabytes() * 1024
    }

    pub fn filename(&self) -> Utf8PathBuf {
        match self.format {
            DiskFormat::Ext4 => Utf8PathBuf::from(format!("{}.ext4", &self.name)),
        }
    }

    pub fn path_with_filename(&self) -> Utf8PathBuf {
        self.path.join(self.filename())
    }

    pub fn setup(&self) -> Result<(), std::io::Error> {
        match self.format {
            DiskFormat::Ext4 => self.setup_ext4(),
        }
    }

    pub fn usage_on_disk(&self) -> Result<u64, std::io::Error> {
        fs::du(self.path_with_filename())
    }

    pub fn setup_ext4(&self) -> Result<(), std::io::Error> {
        fs::dd(self.path_with_filename(), self.size_in_megabytes())?;
        fs::mkfs_ext4(self.path_with_filename())?;
        Ok(())
    }

    pub fn destroy(&self) -> Result<(), std::io::Error> {
        fs::rm_rf(self.path_with_filename())?;
        Ok(())
    }

    pub fn usage_pct(&self) -> Result<u8, std::io::Error> {
        Ok((self.usage_on_disk()? * 100 / self.size_in_kilobytes()) as u8)
    }
}
