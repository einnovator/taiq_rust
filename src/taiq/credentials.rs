use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialMeta {
    pub id: String,
    pub name: String,
    pub provider: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialDraft {
    #[serde(flatten)]
    pub meta: CredentialMeta,
    pub values: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CredentialIndex {
    records: Vec<CredentialMeta>,
}

pub struct CredentialStore {
    path: PathBuf,
    prefix: String,
    index: CredentialIndex,
}

impl CredentialStore {
    pub fn open(config_path: &Path) -> Result<Self> {
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_DATA_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|v| PathBuf::from(v).join(".local/share")))
            .unwrap_or_else(std::env::temp_dir)
            .join("taiq");
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        config_path.hash(&mut hasher);
        let id = format!("{:016x}", hasher.finish());
        let path = base.join(format!("{id}.credentials.json"));
        let index = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            CredentialIndex::default()
        };
        Ok(Self {
            path,
            prefix: format!("taiq/{id}/"),
            index,
        })
    }

    pub fn list(&self) -> &[CredentialMeta] {
        &self.index.records
    }

    pub fn save(&mut self, draft: CredentialDraft) -> Result<()> {
        if draft.meta.id.trim().is_empty()
            || draft.meta.provider.trim().is_empty()
            || draft.meta.name.trim().is_empty()
        {
            bail!("credential id, name, and provider are required");
        }
        if !draft.values.is_object() {
            bail!("credential values must be a YAML object");
        }
        secret::write(
            &self.target(&draft.meta.id),
            &serde_json::to_vec(&draft.values)?,
        )?;
        if let Some(old) = self
            .index
            .records
            .iter_mut()
            .find(|item| item.id == draft.meta.id)
        {
            *old = draft.meta;
        } else {
            self.index.records.push(draft.meta);
        }
        self.persist()
    }

    pub fn get(&self, id: &str) -> Result<Value> {
        if !self.index.records.iter().any(|item| item.id == id) {
            bail!("credential not found: {id}");
        }
        Ok(serde_json::from_slice(&secret::read(&self.target(id))?)?)
    }

    pub fn delete(&mut self, id: &str) -> Result<()> {
        if !self.index.records.iter().any(|item| item.id == id) {
            bail!("credential not found: {id}");
        }
        secret::delete(&self.target(id))?;
        self.index.records.retain(|item| item.id != id);
        self.persist()
    }

    pub fn token_for(&self, provider_or_id: &str) -> Option<String> {
        self.index
            .records
            .iter()
            .rev()
            .filter(|item| item.provider == provider_or_id || item.id == provider_or_id)
            .find_map(|item| {
                let values = self.get(&item.id).ok()?;
                ["api_key", "token", "access_token", "secret"]
                    .into_iter()
                    .find_map(|key| {
                        values
                            .get(key)
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                    })
            })
    }

    fn target(&self, id: &str) -> String {
        format!("{}{id}", self.prefix)
    }

    fn persist(&self) -> Result<()> {
        fs::create_dir_all(
            self.path
                .parent()
                .context("credential index has no parent")?,
        )?;
        fs::write(&self.path, serde_json::to_vec_pretty(&self.index)?)?;
        Ok(())
    }
}

#[cfg(windows)]
mod secret {
    use anyhow::{Context, Result};
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    };

    fn wide(value: &str) -> Vec<u16> {
        OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn write(target: &str, bytes: &[u8]) -> Result<()> {
        let mut name = wide(target);
        let mut user = wide("taiq");
        let mut blob = bytes.to_vec();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: name.as_mut_ptr(),
            UserName: user.as_mut_ptr(),
            CredentialBlobSize: blob.len().try_into()?,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        let ok = unsafe { CredWriteW(&credential, 0) };
        blob.fill(0);
        if ok == 0 {
            return Err(std::io::Error::last_os_error()).context("write Windows credential");
        }
        Ok(())
    }

    pub fn read(target: &str) -> Result<Vec<u8>> {
        let name = wide(target);
        let mut pointer = std::ptr::null_mut();
        if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut pointer) } == 0 {
            return Err(std::io::Error::last_os_error()).context("read Windows credential");
        }
        let data = unsafe {
            let credential = &*pointer;
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
            .to_vec()
        };
        unsafe {
            CredFree(pointer.cast());
        }
        Ok(data)
    }

    pub fn delete(target: &str) -> Result<()> {
        let name = wide(target);
        if unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
            return Err(std::io::Error::last_os_error()).context("delete Windows credential");
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod secret {
    use anyhow::{Result, bail};
    pub fn write(_: &str, _: &[u8]) -> Result<()> {
        bail!("OS credential storage is not available on this platform")
    }
    pub fn read(_: &str) -> Result<Vec<u8>> {
        bail!("OS credential storage is not available on this platform")
    }
    pub fn delete(_: &str) -> Result<()> {
        bail!("OS credential storage is not available on this platform")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn windows_credential_roundtrip() {
        let name = format!("taiq-test-{}", std::process::id());
        secret::write(&name, b"test-secret").unwrap();
        assert_eq!(secret::read(&name).unwrap(), b"test-secret");
        secret::delete(&name).unwrap();
    }
}
