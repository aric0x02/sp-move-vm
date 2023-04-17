use crate::io::balance::{BalanceOp, MasterOfCoinSession};
use crate::io::context::ExecutionContext;
use crate::io::traits::BalanceAccess;
use alloc::vec::Vec;
use anyhow::Error;
use diem_types::account_config;
use move_binary_format::errors::VMResult;
use move_core_types::account_address::AccountAddress;
use move_core_types::effects::{ChangeSet, Event};
use move_core_types::language_storage::{ModuleId, StructTag, CORE_CODE_ADDRESS};
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use move_table_extension::{TableOperation,TableResolver,TableHandle};
use move_core_types::gas_schedule::{InternalGasUnits,GasCarrier};
use move_vm_runtime::native_functions::NativeContextExtensions;
pub struct StateSession<
    'b,
    'r,
    R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error>+TableResolver,
    B: BalanceAccess,
> {
    remote: &'r R,
    context: Option<ExecutionContext>,
    coin_session: MasterOfCoinSession<'b, 'r, B, R>,
}

impl<
        'b,
        'r,
        R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error>+TableResolver,
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
  
}

impl<
        'b,
        'r,
        R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error>+TableResolver,
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
        R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error>+TableResolver,
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
        R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error>+TableResolver,
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
    fn table_size(&self, handle: &TableHandle) -> Result<usize, anyhow::Error>{
        self.remote.table_size(handle)
    }

    fn operation_cost(
        &self,
        op: TableOperation,
        key_size: usize,
        val_size: usize,
    ) -> InternalGasUnits<GasCarrier>{
        self.remote.operation_cost(op,key_size,val_size)
    }
}