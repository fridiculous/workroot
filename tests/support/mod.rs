use std::fs;
use std::path::Path;

pub struct FakeTool;

impl FakeTool {
    pub fn write(dir: &Path, name: &str, script: &str) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join(name);
        fs::write(&path, script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&path)?.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions)?;
        }
        Ok(Self)
    }
}
