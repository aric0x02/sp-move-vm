use crate::io::{
    balance::{BalanceOp, MasterOfCoinSession},
    context::ExecutionContext,
    traits::BalanceAccess,
};
use alloc::vec::Vec;
use anyhow::Error;
use diem_types::account_config;
use move_binary_format::errors::{Location, VMResult};
use move_core_types::{
    account_address::AccountAddress,
    effects::{ChangeSet, Event},
    gas_schedule::{GasCarrier, InternalGasUnits},
    language_storage::{ModuleId, StructTag, TypeTag, CORE_CODE_ADDRESS},
    resolver::{ModuleResolver, ResourceResolver},
};
use move_table_extension::{
    NativeTableContext, TableChangeSet, TableHandle, TableOperation, TableResolver,
};
use move_vm_runtime::native_functions::NativeContextExtensions;
// use move_vm_runtime::native_functions::NativeContextExtensions;
// use crate::types::{Call, Gas, ModuleTx, PublishPackageTx, ScriptTx};
use diem_crypto::{hash::CryptoHash, HashValue};
use diem_crypto_derive::{BCSCryptoHash, CryptoHasher};
use serde::{Deserialize, Serialize};

#[derive(BCSCryptoHash, CryptoHasher, Deserialize, Serialize)]
pub enum SessionId {
    Txn {
        call: Vec<u8>,
        args: Vec<Vec<u8>>,
        type_args: Vec<TypeTag>,
        signers: Vec<AccountAddress>,
    },
    ModuleTx {
        code: Vec<u8>,
        sender: AccountAddress,
    },
    PublishPackageTx {
        modules: Vec<Vec<u8>>,
        address: AccountAddress,
    },
    // For those runs that are not a transaction and the output of which won't be committed.
    Void,
}

impl SessionId {
    pub fn txn(
        call: Vec<u8>,
        args: Vec<Vec<u8>>,
        type_args: Vec<TypeTag>,
        signers: Vec<AccountAddress>,
    ) -> Self {
        Self::Txn {
            call,
            args,
            type_args,
            signers,
        }
    }
    pub fn module_tx(code: Vec<u8>, sender: AccountAddress) -> Self {
        Self::ModuleTx { code, sender }
    }
    pub fn publish_package_tx(modules: Vec<Vec<u8>>, address: AccountAddress) -> Self {
        Self::PublishPackageTx { modules, address }
    }
    pub fn void() -> Self {
        Self::Void
    }

    pub fn as_uuid(&self) -> HashValue {
        self.hash()
    }
}

pub struct StateSession<
    'b,
    'r,
    R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error> + TableResolver,
    B: BalanceAccess,
> {
    remote: &'r R,
    context: Option<ExecutionContext>,
    coin_session: MasterOfCoinSession<'b, 'r, B, R>,
}

impl<
        'b,
        'r,
        R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error> + TableResolver,
        B: BalanceAccess,
    > StateSession<'b, 'r, R, B>
{
    pub(crate) fn new(
        remote: &'r R,
        context: Option<ExecutionContext>,
        coin_session: MasterOfCoinSession<'b, 'r, B, R>,
    ) -> StateSession<'b, 'r, R, B> {
        StateSession {
            remote,
            context,
            coin_session,
        }
    }

    pub fn finish(
        &self,
        (mut changes, events): (ChangeSet, Vec<Event>),
    ) -> VMResult<(ChangeSet, Vec<Event>, Vec<BalanceOp>)> {
        let balance_op = self.coin_session.finish(&mut changes)?;

        Ok((changes, events, balance_op))
    }
    pub fn finish_with_extensions(
        &self,
        (mut changes, events, mut extensions): (ChangeSet, Vec<Event>, NativeContextExtensions),
    ) -> VMResult<(ChangeSet, Vec<Event>, Vec<BalanceOp>, TableChangeSet)> {
        // let (_, _, mut extensions) = self.inner.finish_with_extensions()?;
        let table_context: NativeTableContext = extensions.remove();
        let table_change_set = table_context
            .into_change_set()
            .map_err(|e| e.finish(Location::Undefined))?;
        let balance_op = self.coin_session.finish(&mut changes)?;

        Ok((changes, events, balance_op, table_change_set))
    }
}

impl<
        'b,
        'r,
        R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error> + TableResolver,
        B: BalanceAccess,
    > ModuleResolver for StateSession<'b, 'r, R, B>
{
    type Error = Error;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.remote.get_module(id)
    }
}

impl<
        'b,
        'r,
        R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error> + TableResolver,
        B: BalanceAccess,
    > ResourceResolver for StateSession<'b, 'r, R, B>
{
    type Error = Error;

    fn get_resource(
        &self,
        address: &AccountAddress,
        tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        if tag.address == CORE_CODE_ADDRESS {
            if address == &account_config::diem_root_address() {
                if let Some(ctx) = &self.context {
                    if let Some(blob) = ctx.resolve(tag) {
                        return Ok(Some(blob));
                    }
                }
            }
            if let Some(blob) = self.coin_session.resolve(address, tag)? {
                return Ok(Some(blob));
            }
        }
        self.remote.get_resource(address, tag)
    }
}

impl<
        'b,
        'r,
        R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error> + TableResolver,
        B: BalanceAccess,
    > TableResolver for StateSession<'b, 'r, R, B>
{
    fn resolve_table_entry(
        &self,
        handle: &TableHandle,
        key: &[u8],
    ) -> Result<Option<Vec<u8>>, anyhow::Error> {
        self.remote.resolve_table_entry(handle, key)
    }
    fn table_size(&self, handle: &TableHandle) -> Result<usize, anyhow::Error> {
        self.remote.table_size(handle)
    }

    fn operation_cost(
        &self,
        op: TableOperation,
        key_size: usize,
        val_size: usize,
    ) -> InternalGasUnits<GasCarrier> {
        self.remote.operation_cost(op, key_size, val_size)
    }
}
