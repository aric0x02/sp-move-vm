use crate::io::balance::MasterOfCoin;
use crate::io::context::ExecutionContext;
use crate::io::key::AccessKey;
use crate::io::session::StateSession;
pub use crate::io::traits::{BalanceAccess, Storage};
use alloc::vec::Vec;
use anyhow::Error;
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag};
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use move_table_extension::{TableOperation,TableResolver,TableHandle};
use move_core_types::gas_schedule::{InternalGasUnits,GasCarrier, GasCost};

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
//     pub fn static_state_session<'c, B: BalanceAccess>(
//         &self,
//         context: Option<ExecutionContext>,
//         master_of_coin: &'c MasterOfCoin<B>,
//     ) -> &'static StateSession<'c, '_, State<S>, B> {
// use cell::{Lazy, OnceCell};
//         static static_state_session: OnceCell<StateSession<'b,'r,State<dyn Storage>,dyn BalanceAccess>> = OnceCell::new();

//         static_state_session.set(StateSession::new(self, context, master_of_coin.session(self)));
//         static_state_session.get().unwrap() 
//         // &StateSession::new(self, context, master_of_coin.session(self))
//     }
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
        Ok(self.store.get(AccessKey::from((handle, key)).as_ref()))
    }
    fn table_size(&self, handle: &TableHandle) -> Result<usize, anyhow::Error>{
        Ok(0)
    }

    fn operation_cost(
        &self,
        op: TableOperation,
        key_size: usize,
        val_size: usize,
    ) -> InternalGasUnits<GasCarrier>{
        GasCost::new(key_size as u64,val_size as u64).total()
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
