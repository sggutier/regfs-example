use log::warn;
use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Component, Path, PathBuf},
};
use winreg::RegKey;

mod utils {
    use std::path::{Component, Path};

    pub fn is_virtualization_root(path: &Path) -> bool {
        if let Some(comp) = path.components().next() {
            // some component, must be \ to be root
            comp == Component::RootDir
        } else {
            // no components, still root
            true
        }
    }
}

#[derive(Default, Debug)]
pub struct RegEntry {
    pub name: OsString,
    pub size: u64,
}

impl RegEntry {
    fn new<T: Into<OsString>>(name: T, size: u64) -> Self {
        RegEntry {
            name: name.into(),
            size,
        }
    }
}

#[derive(Default, Debug)]
pub struct RegEntires {
    pub subkeys: Vec<RegEntry>,
    pub values: Vec<RegEntry>,
}

pub struct RegOps {
    keymap: HashMap<OsString, RegKey>,
}

impl RegOps {
    pub fn new() -> RegOps {
        let mut keymap = HashMap::new();
        keymap.insert(
            "HKEY_CLASSES_ROOT".into(),
            RegKey::predef(winreg::enums::HKEY_CLASSES_ROOT),
        );
        keymap.insert(
            "HKEY_CURRENT_USER".into(),
            RegKey::predef(winreg::enums::HKEY_CURRENT_USER),
        );
        keymap.insert(
            "HKEY_LOCAL_MACHINE".into(),
            RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE),
        );
        keymap.insert(
            "HKEY_USERS".into(),
            RegKey::predef(winreg::enums::HKEY_USERS),
        );
        keymap.insert(
            "HKEY_CURRENT_CONFIG".into(),
            RegKey::predef(winreg::enums::HKEY_CURRENT_CONFIG),
        );

        RegOps { keymap }
    }

    pub fn enumerate_key(&self, path: OsString) -> Option<RegEntires> {
        if utils::is_virtualization_root(path.as_ref()) {
            let subkeys = self
                .keymap
                .iter()
                .map(|(n, _)| RegEntry::new(n, 0))
                .collect();

            Some(RegEntires {
                subkeys,
                ..Default::default()
            })
        } else {
            if let Some(subkey) = self.open_key_by_path(path.as_ref()) {
                let subkeys: Vec<RegEntry> = subkey
                    .enum_keys()
                    .filter_map(|s| match s {
                        Ok(s) => Some(RegEntry::new(s, 0)),
                        Err(_) => None,
                    })
                    .collect();
                let values: Vec<RegEntry> = subkey
                    .enum_values()
                    .filter_map(|s| match s {
                        Ok((name, value)) => Some(RegEntry::new(name, value.bytes.len() as u64)),
                        Err(_) => None,
                    })
                    .collect();

                Some(RegEntires { subkeys, values })
            } else {
                None
            }
        }
    }

    pub fn read_value(&self, path: &Path) -> Option<Vec<u8>> {
        let mut parts = path.components();

        if parts.clone().count() <= 1 {
            // only root or empty
            return None;
        }

        let value = parts.next_back().unwrap();
        let subkey = parts.as_path();

        self.open_key_by_path(subkey)
            .and_then(|subkey| subkey.get_raw_value(value).ok())
            .map(|value| value.bytes)
    }

    pub fn does_key_exist(&self, path: &Path) -> bool {
        self.open_key_by_path(path).is_some()
    }

    pub fn does_value_exist(&self, path: &Path) -> Option<usize> {
        self.read_value(path).map(|bytes| bytes.len())
    }

    fn open_key_by_path(&self, path: &Path) -> Option<RegKey> {
        if path.components().count() == 1 {
            if let Some(hkey) = self.keymap.get(path.as_os_str()) {
                Some(RegKey::predef(hkey.raw_handle()))
            } else {
                warn!("open_key_by_path: root key [{:?}] doesn't exist", path);
                None
            }
        } else {
            let mut parts = path.components();
            let rootkey = match parts.next() {
                Some(Component::RootDir) => parts.next().map(|x| x.as_os_str()),
                Some(Component::Normal(part)) => Some(part),
                _ => None,
            }?;
            let subkey = parts.collect::<PathBuf>();
            let root: &RegKey = self.keymap.get(rootkey)?;

            root.open_subkey(subkey).ok()
        }
    }
}

#[test]
fn test_enumerate_key() {
    let ops = RegOps::new();
    let keys = ops
        .enumerate_key("HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft".into())
        .unwrap();

    for key in keys.subkeys {
        dbg!(key.name);
    }
}

#[test]
fn test_does_key_exist() {
    let ops = RegOps::new();

    assert!(ops.does_key_exist(
        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion".as_ref()
    ));
}

#[test]
fn test_read_value() {
    let ops = RegOps::new();
    assert_eq!(ops.read_value("HKEY_LOCAL_MACHINE".as_ref()), None);
    assert_eq!(ops.read_value("".as_ref()), None);
    assert_eq!(
        ops.read_value("HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\CurrentMajorVersionNumber".as_ref()),
        Some(vec![10, 0, 0, 0])
    );
}
