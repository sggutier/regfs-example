use anyhow::{anyhow, Result};
use log::{info, warn};
use prjfs::conv::{RawWStrExt, WStrExt};
use prjfs::guid::guid_to_bytes;
use prjfs::ProviderT;
use std::{collections::HashMap, ffi::OsString, sync::Mutex};
use winapi::{
    shared::{
        guiddef::GUID,
        ntdef::TRUE,
        winerror::{self, HRESULT_FROM_WIN32, S_OK},
    },
    um::{
        projectedfslib::{
            PRJ_CALLBACK_DATA, PRJ_DIR_ENTRY_BUFFER_HANDLE, PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
            PRJ_NOTIFICATION_PARAMETERS, PRJ_PLACEHOLDER_INFO,
        },
        winnt::{HRESULT, LPCWSTR, PCWSTR},
    },
};

use crate::dirinfo::DirInfo;
use crate::regop::RegOps;

#[derive(Default)]
pub struct State {
    enum_sessions: HashMap<Vec<u8>, DirInfo>,
}

pub struct RegFs {
    state: Mutex<State>,
    regops: RegOps,
    readonly: bool,
    context: PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT,
}

impl RegFs {
    pub fn new() -> Self {
        RegFs {
            state: Mutex::new(Default::default()),
            regops: RegOps::new(),
            readonly: true,
            context: std::ptr::null_mut(),
        }
    }
}

impl RegFs {
    fn write_placeholder_info(&self, filepath: LPCWSTR, info: PRJ_PLACEHOLDER_INFO) -> HRESULT {
        unsafe {
            prjfs::sys::PrjWritePlaceholderInfo(
                self.context,
                filepath,
                &info,
                std::mem::size_of_val(&info) as u32,
            )
        }
    }

    fn populate_dir_info_for_path(
        &self,
        path: OsString,
        dirinfo: &mut DirInfo,
        search_expression: OsString,
    ) -> bool {
        let entries = if let Some(entries) = self.regops.enumerate_key(path) {
            entries
        } else {
            return false;
        };

        for subkey in entries.subkeys {
            let result = unsafe {
                prjfs::sys::PrjFileNameMatch(
                    subkey.name.to_wstr().as_ptr(),
                    search_expression.to_wstr().as_ptr(),
                )
            };

            if result == TRUE {
                dirinfo.fill_dir_entry(subkey.name);
            }
        }

        for value in entries.values {
            let result = unsafe {
                prjfs::sys::PrjFileNameMatch(
                    value.name.to_wstr().as_ptr(),
                    search_expression.to_wstr().as_ptr(),
                )
            };

            if result == TRUE {
                dirinfo.fill_file_entry(value.name, value.size as i64);
            }
        }

        true
    }
}

impl ProviderT for RegFs {
    fn get_context_mut(&mut self) -> Option<*mut prjfs::sys::PRJ_NAMESPACE_VIRTUALIZATION_CONTEXT> {
        Some(&mut self.context)
    }

    fn start_dir_enum(
        &self,
        callback_data: &PRJ_CALLBACK_DATA,
        enumeration_id: &GUID,
    ) -> Result<HRESULT> {
        let filepath = callback_data.FilePathName.to_os();
        info!(
            "----> start_dir_enum: Path [{:?}] triggered by [{:?}]",
            filepath,
            callback_data.TriggeringProcessImageFileName.to_os()
        );

        let guid = guid_to_bytes(enumeration_id);
        self.state
            .lock()
            .unwrap()
            .enum_sessions
            .insert(guid, DirInfo::new(filepath));

        info!("<---- start_dir_enum: return 0x0");

        Ok(0)
    }

    fn end_dir_enum(
        &self,
        _callback_data: &PRJ_CALLBACK_DATA,
        enumeration_id: &GUID,
    ) -> Result<HRESULT> {
        info!("----> end_dir_enum");

        let guid = guid_to_bytes(enumeration_id);
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("unable to acquire state"))?;

        state.enum_sessions.remove(&guid);

        info!("<---- end_dir_enum: return 0x0");
        Ok(0)
    }

    fn get_dir_enum(
        &self,
        data: &PRJ_CALLBACK_DATA,
        enumeration_id: &GUID,
        search_expression: PCWSTR,
        handle: PRJ_DIR_ENTRY_BUFFER_HANDLE,
    ) -> Result<HRESULT> {
        let path = data.FilePathName.to_os();
        let search_expression = search_expression.to_os();
        info!(
            "----> get_dir_enum: Path [{:?}] SearchExpression: [{:?}]",
            path, search_expression
        );

        let guid = guid_to_bytes(enumeration_id);
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("unable to acquire state"))?;

        let dirinfo = match state.enum_sessions.get_mut(&guid) {
            Some(session) => session,
            None => return Ok(winerror::E_INVALIDARG),
        };

        if data.Flags & prjfs::sys::PRJ_CB_DATA_FLAG_ENUM_RESTART_SCAN != 0 {
            dirinfo.reset();
        }

        if !dirinfo.filled() {
            if !self.populate_dir_info_for_path(path, dirinfo, search_expression) {
                return Err(anyhow!("failed to get key"));
            }

            dirinfo.sort_entries_and_mark_filled();
        }

        while dirinfo.current_is_valid() {
            let result = unsafe {
                prjfs::sys::PrjFillDirEntryBuffer(
                    dirinfo.current_file_name().as_ptr(),
                    &mut dirinfo.current_basic_info(),
                    handle,
                )
            };

            if result != S_OK {
                break;
            }

            dirinfo.move_next();
        }

        info!("<---- get_dir_enum: return {:08x}", 0);
        Ok(S_OK)
    }

    fn get_placeholder_info(&self, data: &PRJ_CALLBACK_DATA) -> Result<HRESULT> {
        let path = data.FilePathName.to_os();
        info!(
            "----> get_placeholder_info: Path [{:?}] triggered by {:?}]",
            path,
            data.TriggeringProcessImageFileName.to_os()
        );

        let size: Option<i64> = if self.regops.does_key_exist(path.as_ref()) {
            None
        } else if let Some(size) = self.regops.does_value_exist(path.as_ref()) {
            Some(size as i64)
        } else {
            info!(
                "<---- get_place_holder_info: return {:08x}",
                winerror::ERROR_FILE_NOT_FOUND
            );
            return Ok(winerror::HRESULT_FROM_WIN32(winerror::ERROR_FILE_NOT_FOUND));
        };

        let mut placeholder = prjfs::sys::PRJ_PLACEHOLDER_INFO::default();
        if let Some(size) = size {
            placeholder.FileBasicInfo.IsDirectory = false as u8;
            placeholder.FileBasicInfo.FileSize = size;
        } else {
            placeholder.FileBasicInfo.IsDirectory = true as u8;
            placeholder.FileBasicInfo.FileSize = 0;
        }

        let result = self.write_placeholder_info(data.FilePathName, placeholder);

        info!("<---- get_placeholder_info: {:08x}", result);

        Ok(result)
    }

    fn get_file_data(&self, data: &PRJ_CALLBACK_DATA, offset: u64, length: u32) -> Result<HRESULT> {
        let path = data.FilePathName.to_os();
        let process = data.TriggeringProcessImageFileName.to_os();
        info!(
            "----> get_file_data: Path[{:?}] triggered by [{:?}]",
            path, process
        );

        let rawbuffer =
            unsafe { prjfs::sys::PrjAllocateAlignedBuffer(self.context, length as usize) };
        if rawbuffer.is_null() {
            warn!("<---- get_file_data: Could not allocate write buffer.");
            return Ok(winerror::E_OUTOFMEMORY);
        }
        let buffer =
            unsafe { std::slice::from_raw_parts_mut(rawbuffer as *mut u8, length as usize) };

        let hr = if let Some(bytes) = self.regops.read_value(path.as_ref()) {
            buffer.copy_from_slice(&bytes);
            unsafe {
                prjfs::sys::PrjWriteFileData(
                    self.context,
                    &data.DataStreamId,
                    rawbuffer,
                    offset,
                    length,
                )
            }
        } else {
            winerror::HRESULT_FROM_WIN32(winerror::ERROR_FILE_NOT_FOUND)
        };

        unsafe {
            prjfs::sys::PrjFreeAlignedBuffer(rawbuffer);
        }
        info!("<---- get_file_data: return {:08x}", hr);
        Ok(hr)
    }

    fn notify(
        &self,
        data: &PRJ_CALLBACK_DATA,
        _is_directory: bool,
        notification_type: prjfs::sys::PRJ_NOTIFICATION,
        destination_file_name: PCWSTR,
        _parameters: &PRJ_NOTIFICATION_PARAMETERS,
    ) -> Result<HRESULT> {
        let filepath = data.FilePathName.to_os();
        let process = data.TriggeringProcessImageFileName.to_os();
        info!(
            "---> notify: Path [{:?}] triggered by [{:?}]",
            filepath, process
        );
        info!("--- Notification: 0x{:08x}", notification_type);

        match notification_type {
            prjfs::sys::PRJ_NOTIFICATION_FILE_OPENED => Ok(S_OK),
            prjfs::sys::PRJ_NOTIFICATION_FILE_HANDLE_CLOSED_FILE_MODIFIED
            | prjfs::sys::PRJ_NOTIFICATION_FILE_OVERWRITTEN => {
                info!(" ----- [{:?}] was modified", filepath);
                Ok(S_OK)
            }
            prjfs::sys::PRJ_NOTIFY_NEW_FILE_CREATED => {
                info!(" ----- [{:?}] was created", filepath);
                Ok(S_OK)
            }
            prjfs::sys::PRJ_NOTIFY_FILE_RENAMED => {
                info!(
                    " ----- [{:?}] -> [{:?}]",
                    filepath,
                    destination_file_name.to_os()
                );
                Ok(S_OK)
            }
            prjfs::sys::PRJ_NOTIFY_FILE_HANDLE_CLOSED_FILE_DELETED => {
                info!(" ----- [{:?}] was deleted", filepath);
                Ok(S_OK)
            }
            prjfs::sys::PRJ_NOTIFICATION_PRE_RENAME => {
                if self.readonly {
                    info!(" ----- rename request for [{:?}] was rejected", filepath);
                    Ok(HRESULT_FROM_WIN32(winerror::ERROR_ACCESS_DENIED))
                } else {
                    info!(" ----- rename request for [{:?}]", filepath);
                    Ok(S_OK)
                }
            }
            prjfs::sys::PRJ_NOTIFICATION_PRE_DELETE => {
                if self.readonly {
                    info!(" ----- delete request for [{:?}] was rejected", filepath);
                    Ok(HRESULT_FROM_WIN32(winerror::ERROR_ACCESS_DENIED))
                } else {
                    info!(" ----- delete request for [{:?}]", filepath);
                    Ok(S_OK)
                }
            }
            prjfs::sys::PRJ_NOTIFICATION_FILE_PRE_CONVERT_TO_FULL => Ok(S_OK),
            t => {
                warn!("notify: Unexpected notification: 0x{:08x}", t);
                Ok(S_OK)
            }
        }
    }

    fn query_file_name(&self, _data: &PRJ_CALLBACK_DATA) -> Result<HRESULT> {
        Ok(S_OK)
    }

    fn cancel_command(&self, _data: &PRJ_CALLBACK_DATA) -> Result<()> {
        Ok(())
    }
}
