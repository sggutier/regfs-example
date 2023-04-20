use prjfs::conv::{WStr, WStrExt};
use std::{
    cmp::Ordering,
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Debug)]
struct DirEntry {
    filename: OsString,
    is_directory: bool,
    size: i64,
}

#[derive(Default, Debug)]
pub struct DirInfo {
    #[allow(dead_code)]
    path: PathBuf,
    index: usize,
    filled: bool,
    entries: Vec<DirEntry>,
}

impl DirInfo {
    pub fn new<T: AsRef<Path>>(path: T) -> Self {
        DirInfo {
            path: path.as_ref().to_owned(),
            ..Default::default()
        }
    }

    pub fn reset(&mut self) {
        self.index = 0;
        self.filled = false;
        self.entries = Vec::new();
    }

    pub fn filled(&self) -> bool {
        self.filled
    }

    pub fn current_is_valid(&self) -> bool {
        self.index < self.entries.len()
    }

    pub fn current_file_name(&self) -> WStr {
        self.entries[self.index].filename.to_wstr()
    }

    pub fn current_basic_info(&self) -> prjfs::sys::PRJ_FILE_BASIC_INFO {
        let mut info = prjfs::sys::PRJ_FILE_BASIC_INFO::default();
        info.IsDirectory = self.entries[self.index].is_directory as u8;
        info.FileSize = self.entries[self.index].size;
        info
    }

    pub fn move_next(&mut self) -> bool {
        self.index += 1;
        self.index < self.entries.len()
    }

    pub fn fill_dir_entry(&mut self, name: OsString) {
        self.fill_item_entry(name, 0, true);
    }

    pub fn fill_file_entry(&mut self, name: OsString, size: i64) {
        self.fill_item_entry(name, size, false);
    }

    fn fill_item_entry(&mut self, filename: OsString, size: i64, is_directory: bool) {
        self.entries.push(DirEntry {
            filename,
            size,
            is_directory,
        });
    }

    pub fn sort_entries_and_mark_filled(&mut self) {
        self.filled = true;

        self.entries.sort_by(|a, b| {
            let result = unsafe {
                prjfs::sys::PrjFileNameCompare(
                    a.filename.to_wstr().as_ptr(),
                    b.filename.to_wstr().as_ptr(),
                )
            };

            if result < 0 {
                Ordering::Less
            } else if result == 0 {
                Ordering::Equal
            } else {
                Ordering::Greater
            }
        });
    }
}
