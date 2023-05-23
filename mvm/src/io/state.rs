pub use crate::io::traits::{BalanceAccess, Storage};
use crate::io::{
    balance::MasterOfCoin, context::ExecutionContext, key::AccessKey, session::StateSession,
};
use alloc::vec::Vec;
use anyhow::Error;
use move_core_types::{
    account_address::AccountAddress,
    gas_schedule::{GasCarrier, GasCost, InternalGasUnits},
    language_storage::{ModuleId, StructTag},
    resolver::{ModuleResolver, ResourceResolver},
};
use move_table_extension::{TableHandle, TableOperation, TableResolver};

pub struct State<S: Storage> {
    store: S,
}

impl<S: Storage> State<S> {
    pub fn new(store: S) -> State<S> {
        State { store }
    }

    pub fn state_session<'c, B: BalanceAccess>(
        &self,
        context: Option<ExecutionContext>,
        master_of_coin: &'c MasterOfCoin<B>,
    ) -> StateSession<'c, '_, State<S>, B> {
        StateSession::new(self, context, master_of_coin.session(self))
    }
}

impl<S: Storage> ModuleResolver for State<S> {
    type Error = Error;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.store.get(AccessKey::from(module_id).as_ref()))
    }
}

impl<S: Storage> ResourceResolver for State<S> {
    type Error = Error;

    fn get_resource(
        &self,
        address: &AccountAddress,
        typ: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.store.get(AccessKey::from((address, typ)).as_ref()))
    }
}

impl<S: Storage> TableResolver for State<S> {
    fn resolve_table_entry(
        &self,
        handle: &TableHandle,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>, anyhow::Error> {
        log::warn!("Failed to key=====state===={:?}===={:?}===:{:?}",handle,key, self.store.get(AccessKey::from((handle, key)).as_ref()));
        Ok(self.store.get(AccessKey::from((handle, key)).as_ref()))
    }
    fn table_size(&self, _handle: &TableHandle) -> Result<usize, anyhow::Error> {
        Ok(0)
    }

    fn operation_cost(
        &self,
        _op: TableOperation,
        key_size: usize,
        val_size: usize,
    ) -> InternalGasUnits<GasCarrier> {
        GasCost::new(key_size as u64, val_size as u64).total()
    }
}

impl<S: Storage> WriteEffects for State<S> {
    fn delete(&self, key: AccessKey) {
        self.store.remove(key.as_ref());
    }

    fn insert(&self, key: AccessKey, blob: Vec<u8>) {
        self.store.insert(key.as_ref(), &blob);
    }
}

pub trait WriteEffects {
    fn delete(&self, path: AccessKey);
    fn insert(&self, path: AccessKey, blob: Vec<u8>);
}
