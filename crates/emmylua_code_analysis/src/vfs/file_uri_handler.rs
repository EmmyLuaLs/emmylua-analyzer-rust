use lsp_types::Uri;
use percent_encoding::percent_decode_str;
use std::path::PathBuf;
use std::str::FromStr;
use url::Url;

pub fn file_path_to_uri(path: &PathBuf) -> Option<Uri> {
    Url::from_file_path(path)
        .ok()
        .and_then(|url| Uri::from_str(url.as_str()).ok())
}

pub fn uri_to_file_path(uri: &Uri) -> Option<PathBuf> {
    let url = Url::parse(uri.as_str()).ok()?;
    if url.scheme() != "file" {
        return None;
    }

    let decoded_path = percent_decode_str(url.path())
        .decode_utf8()
        .ok()?
        .to_string();

    let decoded_path = if cfg!(windows) {
        let mut windows_decoded_path = decoded_path.trim_start_matches('/').replace('\\', "/");
        if windows_decoded_path.len() >= 2 && windows_decoded_path.chars().nth(1) == Some(':') {
            let drive = windows_decoded_path.chars().next()?.to_ascii_uppercase();
            windows_decoded_path.replace_range(..2, &format!("{}:", drive));
        }

        windows_decoded_path
    } else {
        decoded_path
    };

    Some(PathBuf::from(decoded_path))
}

#[cfg(test)]
mod tests {
    use std::{path::Path, str::FromStr};

    use lsp_types::Uri;

    use crate::{file_path_to_uri, uri_to_file_path, Emmyrc, Vfs};

    fn create_vfs() -> Vfs {
        let mut vfs = Vfs::new();
        vfs.update_config(Emmyrc::default().into());
        vfs
    }

    #[test]
    fn test_basic() {
        let mut vfs = create_vfs();

        let uri = Uri::from_str("file:///C:/Users/username/Documents/test.lua").unwrap();
        let id = vfs.file_id(&uri);
        assert_eq!(id.id, 0);
        let id_another = vfs.get_file_id(&uri).unwrap();
        assert_eq!(id_another, id);
        let uri2 = Uri::from_str("file:///C:/Users/username/Documents/test2.lua").unwrap();

        let id2 = vfs.file_id(&uri2);
        assert_eq!(id2.id, 1);
        assert!(id2 != id);

        vfs.set_file_content(&uri, Some("content".to_string()));
        let content = vfs.get_file_content(&id).unwrap();
        assert_eq!(content, "content");

        let content2 = vfs.get_file_content(&id2);
        assert!(content2.is_none());
    }

    #[test]
    fn test_clear_file() {
        let mut vfs = create_vfs();
        let uri = Uri::from_str("file:///C:/Users/username/Documents/test.lua").unwrap();
        let id = vfs.file_id(&uri);
        vfs.set_file_content(&uri, Some("content".to_string()));
        let content = vfs.get_file_content(&id).unwrap();
        assert_eq!(content, "content");

        vfs.set_file_content(&uri, None);
        let content = vfs.get_file_content(&id);
        assert!(content.is_none());
    }

    #[test]
    fn test_file_path_to_uri() {
        let mut vfs = create_vfs();
        if cfg!(windows) {
            let uri = Uri::from_str("file:///C:/Users/username/Documents/test.lua").unwrap();
            let id = vfs.file_id(&uri);
            let path = Path::new("C:/Users/username/Documents/test.lua");
            let uri2 = file_path_to_uri(&path.into()).unwrap();
            assert_eq!(uri2, uri);
            let id2 = vfs.file_id(&uri2);
            assert_eq!(id2, id);
        }
    }

    #[test]
    fn test_uri_to_file_path() {
        if cfg!(windows) {
            let uri = Uri::from_str("file:///C:/Users/username/Documents/test.lua").unwrap();
            let path2 = uri_to_file_path(&uri).unwrap();
            assert_eq!(path2, Path::new("C:/Users/username/Documents/test.lua"));

            let windows_path = Path::new("C:\\Users\\username\\Documents\\test.lua");
            assert_eq!(path2, windows_path);

            let uri =
                Uri::from_str("file:///c%3A/Users//username/Desktop/learn/test%20main/test.lua")
                    .unwrap();
            let path = uri_to_file_path(&uri).unwrap();
            let path2 = Path::new("C:/Users//username/Desktop/learn/test main/test.lua");
            assert_eq!(path, path2);
        }
    }

    #[test]
    fn test_relative_path() {
        #[cfg(windows)]
        {
            let workspace = Path::new("C:/Users\\username/Documents");
            let uri = Uri::from_str("file:///C:/Users/username/Documents/test.lua").unwrap();
            let file_path = uri_to_file_path(&uri).unwrap();
            let relative_path = file_path.strip_prefix(workspace).unwrap();
            assert_eq!(relative_path, Path::new("test.lua"));
            let file_path2 = Path::new("C:\\Users\\username/Documents\\test.lua");
            let relative_path2 = file_path2.strip_prefix(workspace).unwrap();
            assert_eq!(relative_path2, Path::new("test.lua"));
        }
    }

    #[test]
    fn test_chinese_path() {
        #[cfg(windows)]
        {
            let uri = Uri::from_str("file:///c%3a/%E6%96%B0%E5%BB%BA%E6%96%87%E4%BB%B6%E5%A4%B9")
                .unwrap();
            let path = uri_to_file_path(&uri).unwrap();
            let result_path = Path::new("c:/新建文件夹");
            assert_eq!(path, result_path);
        }
    }
}
