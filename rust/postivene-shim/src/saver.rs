//! A copy of an attachment where the reader can find it again.
//!
//! Everything a chat receives lives in the core's blob directory, which is
//! the app's own and goes with it. Saving is one copy into a folder the
//! platform indexes -- Pictures, Videos, Downloads -- under the name the
//! sender gave the file, or that name and a number when it is taken.

use qmetaobject::*;

use crate::chat::local_path;

/// Copies a file out of the core's blob directory.
///
/// ```qml
/// FileSaver { id: saver; onSaved: notice.show(qsTr("Saved")) }
/// MenuItem { onClicked: saver.save(page.fileUrl, StandardPaths.pictures) }
/// ```
#[derive(QObject, Default)]
pub struct FileSaver {
    base: qt_base_class!(trait QObject),

    /// Copy the file at `file_url` -- a `file://` URL or a plain path --
    /// into `folder`, which is made if it does not exist. Answers on
    /// `saved` or `error`.
    pub save: qt_method!(fn(&mut self, file_url: QString, folder: QString)),
    /// The copy is at `path`.
    pub saved: qt_signal!(path: QString),
    /// The copy could not be made. The message names what went wrong.
    pub error: qt_signal!(message: QString),
}

impl FileSaver {
    /// Copy the file into the folder.
    pub fn save(&mut self, file_url: QString, folder: QString) {
        match copy_into(&local_path(&file_url.to_string()), &folder.to_string()) {
            Ok(path) => self.saved(path.into()),
            Err(message) => self.error(message.into()),
        }
    }
}

/// Copy `source` into `folder` under its own name, or that name and a
/// number when a different file already has it. Answers with the path
/// written.
pub(crate) fn copy_into(source: &str, folder: &str) -> Result<String, String> {
    if source.is_empty() {
        return Err("nothing to save".to_string());
    }
    if folder.is_empty() {
        return Err("nowhere to save to".to_string());
    }
    let source = std::path::Path::new(source);
    let name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("{} has no file name", source.display()))?;
    let folder = std::path::Path::new(folder);
    std::fs::create_dir_all(folder)
        .map_err(|err| format!("cannot create {}: {err}", folder.display()))?;
    let target = free_name(folder, &name);
    std::fs::copy(source, &target).map_err(|err| {
        format!(
            "cannot copy {} to {}: {err}",
            source.display(),
            target.display()
        )
    })?;
    Ok(target.to_string_lossy().into_owned())
}

/// `name`, or `name (2)`, `name (3)`... before the extension: the first
/// that is not already in the folder.
fn free_name(folder: &std::path::Path, name: &str) -> std::path::PathBuf {
    let first = folder.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, extension) = match name.rfind('.') {
        // A leading dot is a hidden file's name, not an extension.
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    };
    (2..=u32::MAX)
        .map(|number| folder.join(format!("{stem} ({number}){extension}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::copy_into;

    #[test]
    fn a_saved_file_keeps_its_name_and_a_second_copy_gets_a_number() {
        let temp = std::env::temp_dir().join(format!("postivene-saver-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).expect("temp dir");
        let source = temp.join("holiday photo.jpg");
        std::fs::write(&source, b"jpeg").expect("write source");
        let pictures = temp.join("Pictures");

        let first =
            copy_into(&source.to_string_lossy(), &pictures.to_string_lossy()).expect("first copy");
        assert_eq!(first, pictures.join("holiday photo.jpg").to_string_lossy());
        assert_eq!(std::fs::read(&first).expect("read copy"), b"jpeg");

        let second =
            copy_into(&source.to_string_lossy(), &pictures.to_string_lossy()).expect("second copy");
        assert_eq!(
            second,
            pictures.join("holiday photo (2).jpg").to_string_lossy()
        );

        // A URL, as the page holds the file, and a name with no extension.
        let plain = temp.join("notes");
        std::fs::write(&plain, b"n").expect("write plain");
        let url = format!("file://{}", plain.display());
        let saved = copy_into(&crate::chat::local_path(&url), &pictures.to_string_lossy())
            .expect("copy from a url");
        assert_eq!(saved, pictures.join("notes").to_string_lossy());

        assert!(copy_into("", &pictures.to_string_lossy()).is_err());
        assert!(copy_into(&source.to_string_lossy(), "").is_err());
        assert!(copy_into(
            &temp.join("missing.png").to_string_lossy(),
            &pictures.to_string_lossy()
        )
        .is_err());
        let _ = std::fs::remove_dir_all(&temp);
    }
}
