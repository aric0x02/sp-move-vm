use alloc::vec::Vec;
use core::convert::TryInto;
use diem_types::access_path::AccessPath;
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag},
};
use move_table_extension::TableHandle;
pub struct AccessKey(Vec<u8>);

pub enum KeyType {
    Resource,
    Module,
    TableItem,
}

impl AccessKey {
    pub fn new(path: AccessPath, k_type: KeyType) -> AccessKey {
        match k_type {
            KeyType::Resource => {
                let mut key = Vec::with_capacity(AccountAddress::LENGTH + path.path.len());
                key.extend_from_slice(path.address.as_ref());
                key.extend_from_slice(path.path.as_ref());
                AccessKey(key)
            }
            KeyType::Module => AccessKey(path.path),
            KeyType::TableItem => AccessKey(path.path),
        }
    }
}

impl From<(&AccountAddress, &StructTag)> for AccessKey {
    fn from((addr, tag): (&AccountAddress, &StructTag)) -> Self {
        let tag = tag.access_vector();
        let mut key = Vec::with_capacity(AccountAddress::LENGTH + tag.len());
        key.extend_from_slice(addr.as_ref());
        key.extend_from_slice(&tag);
        AccessKey(key)
    }
}

impl From<&ModuleId> for AccessKey {
    fn from(id: &ModuleId) -> Self {
        AccessKey(id.access_vector())
    }
}

impl From<(&TableHandle, &[u8])> for AccessKey {
    fn from((handle, th_key): (&TableHandle, &[u8])) -> Self {
        let handle_bytes: [u8; 16] = handle.0.to_be_bytes().try_into().unwrap();
        let handle_bytes: &[u8] = &handle_bytes;
        let mut key = Vec::with_capacity(handle_bytes.len() + th_key.len());
        key.extend_from_slice(handle_bytes);
        key.extend_from_slice(th_key);
        AccessKey(key)
    }
}
impl AsRef<[u8]> for AccessKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
