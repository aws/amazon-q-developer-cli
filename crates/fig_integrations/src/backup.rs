use std::path::{
    Path,
    PathBuf,
};

use fig_util::directories;

pub fn backup_file(path: impl AsRef<Path>, backup_dir: Option<impl Into<PathBuf>>) -> std::io::Result<()> {
    let pathref = path.as_ref();
    if pathref.exists() {
        let name: String = pathref.file_name().unwrap().to_string_lossy().into_owned();
        let dir = match backup_dir {
            Some(dir) => dir.into(),
            None => directories::utc_backup_dir().unwrap(),
        };
        std::fs::create_dir_all(&dir)?;
        std::fs::copy(path, dir.join(name).as_path())?;
    }

    Ok(())
}
