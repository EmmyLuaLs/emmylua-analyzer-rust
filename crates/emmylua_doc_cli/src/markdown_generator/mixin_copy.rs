use std::path::{Path, PathBuf};
use std::fs;
use std::io;

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &dest)?;
        } else {
            fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

pub fn mixin_copy(output: &Path, mixin: PathBuf) -> Option<()> {
    match copy_dir_all(&mixin, output) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to copy mixin: {}", e);
            return None;
        }
    }
    Some(())
}
