use alloc::vec::Vec;

use anyhow::{anyhow, Error};

use diem_types::on_chain_config::VMConfig;
use move_binary_format::{
    errors::{Location, VMError, VMResult},
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress,
    effects::{ChangeSet, Event},
    gas_schedule::{
        CostTable, InternalGasUnits, {AbstractMemorySize, GasAlgebra, GasUnits},
    },
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag, CORE_CODE_ADDRESS},
    resolver::{ModuleResolver, ResourceResolver},
    vm_status::{StatusCode, VMStatus},
};
use move_vm_runtime::{
    move_vm::MoveVM,
    native_functions::{NativeContextExtensions, NativeFunctionTable},
    session::Session,
};
// use move_vm_runtime::session::Session;

// use cell::{Lazy, OnceCell};
// use core::marker::Sync;
use crate::{
    abi::ModuleAbi,
    gas_schedule::cost_table,
    io::{
        balance::{BalanceOp, MasterOfCoin},
        context::ExecutionContext,
        key::AccessKey,
        state::{State, WriteEffects},
        traits::{BalanceAccess, EventHandler, Storage},
    },
    types::{Call, Gas, ModuleTx, PublishPackageTx, ScriptTx, VmResult},
    {StateAccess, Vm},
};
// use move_binary_format::CompiledModule;
// use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use move_vm_types::gas_schedule::GasStatus;
// use core::hash::Hash;
// use move_vm_runtime::native_functions::{NativeContextExtensions, NativeFunctionTable};
// use move_core_types::gas_schedule::{GasCarrier, GasCost};
use move_table_extension::{TableResolver,TableHandle,TableChangeSet};//TableOperation,
// use alloc::boxed::Box;

// static static_state: OnceCell<State<(dyn Storage +Sync+ 'static)>> = OnceCell::new();
pub fn pont_natives(move_std_addr: AccountAddress) -> NativeFunctionTable {
    move_stdlib::natives::all_natives(move_std_addr)
        .into_iter()
        .chain(move_table_extension::table_natives(move_std_addr))
        .collect()
}

/// MoveVM.
pub struct Mvm<S, E, B>
where
    S: Storage,
    E: EventHandler,
    B: BalanceAccess,
{
    vm: MoveVM,
    cost_table: CostTable,
    state: State<S>,
    event_handler: E,
    master_of_coin: MasterOfCoin<B>,
}

impl<S, E, B> Mvm<S, E, B>
where
    S: Storage,
    E: EventHandler,
    B: BalanceAccess,
{
    /// Creates a new move vm with given store and event handler.
    pub fn new(store: S, event_handler: E, balance: B) -> Result<Mvm<S, E, B>, Error> {
        let vm_config = VMConfig {
            gas_schedule: cost_table(),
        };

        Self::new_with_config(store, event_handler, balance, vm_config)
    }

    pub(crate) fn new_with_config(
        store: S,
        event_handler: E,
        balance: B,
        config: VMConfig,
    ) -> Result<Mvm<S, E, B>, Error> {
        Ok(Mvm {
            vm: MoveVM::new(pont_natives(CORE_CODE_ADDRESS)).map_err(|err| {
                let (code, _, msg, _, _, _) = err.all_data();
                anyhow!("Error code:{:?}: msg: '{}'", code, msg.unwrap_or_default())
            })?,
            cost_table: config.gas_schedule,
            state: State::new(store),
            event_handler,
            master_of_coin: MasterOfCoin::new(balance),
        })
    }
    pub(crate) fn execute_function(
        &self,
        sender: AccountAddress,
        gas: Gas,
        module: &ModuleId,
        function_name: &IdentStr,
        ty_args: Vec<TypeTag>,
        args: Vec<Vec<u8>>,
        context: Option<ExecutionContext>,
    ) -> VmResult {
        let state_session = self.state.state_session(context, &self.master_of_coin);
        let mut session = self.vm.new_session(&state_session);
        let mut cost_strategy =
            GasStatus::new(&self.cost_table, GasUnits::new(gas.max_gas_amount()));

        let result =
            session.execute_function(module, function_name, ty_args, args, &mut cost_strategy);

        self.handle_vm_result(
            sender,
            cost_strategy,
            gas,
            result.and_then(|_| session.finish().map(|(ws, e)| (ws, e, vec![]))),
            false,
        )
    }

    /// Stores write set into storage and handle events.
    fn handle_tx_effects_with_extensions(
        &self,
        tx_effects: (ChangeSet, Vec<Event>, Vec<BalanceOp>,TableChangeSet),
    ) -> Result<(), VMError> {
        let (change_set, events, balance_op,table_change_set) = tx_effects;
        self.handle_tx_effects((change_set, events, balance_op))?;
        for (handle, change) in table_change_set.changes {
            for (key, value_op) in change.entries {
                let state_key = AccessKey::from((&handle, &key[..]));
                match value_op {
                    None => {
                        self.state.delete(state_key);
                    }
                    Some(blob) => {
                        self.state.insert(state_key, blob);
                    }
                }
            }
        }
        Ok(())
    }
    /// Stores write set into storage and handle events.
    fn handle_tx_effects(
        &self,
        tx_effects: (ChangeSet, Vec<Event>, Vec<BalanceOp>),
    ) -> Result<(), VMError> {
        let (change_set, events, balance_op) = tx_effects;

        for (addr, acc) in change_set.accounts {
            for (ident, val) in acc.modules {
                let key = AccessKey::from(&ModuleId::new(addr, ident));
                match val {
                    None => {
                        self.state.delete(key);
                    }
                    Some(blob) => {
                        self.state.insert(key, blob);
                    }
                }
            }
            for (tag, val) in acc.resources {
                let key = AccessKey::from((&addr, &tag));
                match val {
                    None => {
                        self.state.delete(key);
                    }
                    Some(blob) => {
                        self.state.insert(key, blob);
                    }
                }
            }
        }

        for (guid, seq_num, ty_tag, msg) in events {
            self.event_handler.on_event(guid, seq_num, ty_tag, msg);
        }

        for op in balance_op.into_iter() {
            self.master_of_coin.update_balance(op);
        }

        Ok(())
    }

    /// Handle vm result and return transaction status code.    
    fn handle_vm_result_with_extensions(
        &self,
        sender: AccountAddress,
        cost_strategy: GasStatus,
        gas_meta: Gas,
        result: Result<(ChangeSet, Vec<Event>, Vec<BalanceOp>,TableChangeSet), VMError>,
        dry_run: bool,
    ) -> VmResult{
         let gas_used = GasUnits::new(gas_meta.max_gas_amount)
            .sub(cost_strategy.remaining_gas())
            .get();

        if dry_run {
            return match result {
                Ok(_) => VmResult::new(StatusCode::EXECUTED, None, None, gas_used),
                Err(err) => VmResult::new(
                    err.major_status(),
                    err.sub_status(),
                    Some(err.location().clone()),
                    gas_used,
                ),
            };
        }

        match result.and_then(|e| self.handle_tx_effects_with_extensions(e)) {
            Ok(_) => VmResult::new(StatusCode::EXECUTED, None, None, gas_used),
            Err(err) => {
                let status = err.major_status();
                let sub_status = err.sub_status();
                let loc = err.location().clone();
                if let Err(err) = self.emit_vm_status_event(sender, err.into_vm_status()) {
                    log::warn!("Failed to emit vm status event:{:?}", err);
                }
                VmResult::new(status, sub_status, Some(loc), gas_used)
            }
        }
    }
    /// Handle vm result and return transaction status code.
    fn handle_vm_result(
        &self,
        sender: AccountAddress,
        cost_strategy: GasStatus,
        gas_meta: Gas,
        result: Result<(ChangeSet, Vec<Event>, Vec<BalanceOp>), VMError>,
        dry_run: bool,
    ) -> VmResult {
        let gas_used = GasUnits::new(gas_meta.max_gas_amount)
            .sub(cost_strategy.remaining_gas())
            .get();

        if dry_run {
            return match result {
                Ok(_) => VmResult::new(StatusCode::EXECUTED, None, None, gas_used),
                Err(err) => VmResult::new(
                    err.major_status(),
                    err.sub_status(),
                    Some(err.location().clone()),
                    gas_used,
                ),
            };
        }

        match result.and_then(|e| self.handle_tx_effects(e)) {
            Ok(_) => VmResult::new(StatusCode::EXECUTED, None, None, gas_used),
            Err(err) => {
                let status = err.major_status();
                let sub_status = err.sub_status();
                let loc = err.location().clone();
                if let Err(err) = self.emit_vm_status_event(sender, err.into_vm_status()) {
                    log::warn!("Failed to emit vm status event:{:?}", err);
                }
                VmResult::new(status, sub_status, Some(loc), gas_used)
            }
        }
    }

    fn emit_vm_status_event(&self, sender: AccountAddress, status: VMStatus) -> Result<(), Error> {
        let tag = TypeTag::Struct(StructTag {
            address: CORE_CODE_ADDRESS,
            module: Identifier::new("VMStatus").unwrap(),
            name: Identifier::new("VMStatus").unwrap(),
            type_params: vec![],
        });

        let msg = bcs::to_bytes(&status)
            .map_err(|err| Error::msg(format!("Failed to generate event message: {:?}", err)))?;

        let mut guid = 0_u64.to_le_bytes().to_vec();
        guid.extend(&sender.to_u8());
        self.event_handler.on_event(guid, 0, tag, msg);
        Ok(())
    }

    fn _publish_module<R>(
        &self,
        session: &mut Session<'_, '_, R>,
        module: Vec<Vec<u8>>,
        sender: AccountAddress,
        cost_strategy: &mut GasStatus,
    ) -> VMResult<()>
    where
        R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error>,
    {
        cost_strategy.charge_intrinsic_gas(AbstractMemorySize::new(module.len() as u64))?;
        let result = session.publish_module_bundle(module, sender, cost_strategy);
        Self::charge_global_write_gas_usage(cost_strategy, session, &sender)?;
        result
    }

    fn charge_global_write_gas_usage<R>(
        cost_strategy: &mut GasStatus,
        session: &mut Session<'_, '_, R>,
        sender: &AccountAddress,
    ) -> VMResult<()>
    where
        R: ModuleResolver<Error = Error> + ResourceResolver<Error = Error>,
    {
        let total_cost = session.num_mutated_accounts(sender)
            * cost_strategy
                .cost_table()
                .gas_constants
                .global_memory_per_byte_write_cost
                .mul(
                    cost_strategy
                        .cost_table()
                        .gas_constants
                        .default_account_size,
                )
                .get();
        cost_strategy
            .deduct_gas(InternalGasUnits::new(total_cost))
            .map_err(|p_err| p_err.finish(Location::Undefined))
    }
}

impl<S, E, B> Vm for Mvm<S, E, B>
where
    S: Storage,
    E: EventHandler,
    B: BalanceAccess,
{
    fn publish_module(&self, gas: Gas, module: ModuleTx, dry_run: bool) -> VmResult {
        let (module, sender) = module.into_inner();
        let mut cost_strategy =
            GasStatus::new(&self.cost_table, GasUnits::new(gas.max_gas_amount()));
        // let mut session = self.vm.new_session(&self.state);
        let mut extensions = NativeContextExtensions::default();
        let txn_hash: [u8; 32] =
            crate::io::session::SessionId::module_tx(module.clone(), sender.clone())
                .as_uuid()
                .to_vec()
                .try_into()
                .expect("HashValue should convert to [u8; 32]");
        let _txn_hash: u128 = txn_hash.iter().fold(0, |mut a, &b| {
            a <<= 8;
            a += b as u128;
            a
        });
        extensions.add(move_table_extension::NativeTableContext::new(
            _txn_hash,
            &self.state,
        ));
        let mut session = self.vm.new_session_with_extensions(&self.state, extensions);
        let result = self
            ._publish_module(&mut session, vec![module], sender, &mut cost_strategy)
            .and_then(|_| session.finish().map(|(ws, e)| (ws, e, vec![])));

        self.handle_vm_result(sender, cost_strategy, gas, result, dry_run)
    }

    fn publish_module_package(
        &self,
        gas: Gas,
        package: PublishPackageTx,
        dry_run: bool,
    ) -> VmResult {
        let (modules, sender) = package.into_inner();
        let mut cost_strategy =
            GasStatus::new(&self.cost_table, GasUnits::new(gas.max_gas_amount()));

        // let mut session = self.vm.new_session(&self.state);

        let mut extensions = NativeContextExtensions::default();
        let txn_hash: [u8; 32] =
            crate::io::session::SessionId::publish_package_tx(modules.clone(), sender.clone())
                .as_uuid()
                .to_vec()
                .try_into()
                .expect("HashValue should convert to [u8; 32]");
        let _txn_hash: u128 = txn_hash.iter().fold(0, |mut a, &b| {
            a <<= 8;
            a += b as u128;
            a
        });
        extensions.add(move_table_extension::NativeTableContext::new(
            _txn_hash,
            &self.state,
        ));
        let mut session = self.vm.new_session_with_extensions(&self.state, extensions);

        let result = self
            ._publish_module(&mut session, modules, sender, &mut cost_strategy)
            .and_then(|_| session.finish().map(|(ws, e)| (ws, e, vec![])));

        self.handle_vm_result(sender, cost_strategy, gas, result, dry_run)
    }

    fn execute_script(
        &self,
        gas: Gas,
        context: ExecutionContext,
        tx: ScriptTx,
        dry_run: bool,
    ) -> VmResult {
        let (script, args, type_args, senders) = tx.into_inner();

        let state_session = self
            .state
            .state_session(Some(context), &self.master_of_coin);
        // let mut vm_session = self.vm.new_session(&state_session);

        let mut extensions = NativeContextExtensions::default();
        let txn_hash: [u8; 32] = crate::io::session::SessionId::txn(
            bcs::to_bytes(&script).unwrap(),
            args.clone(),
            type_args.clone(),
            senders.clone(),
        )
        .as_uuid()
        .to_vec()
        .try_into()
        .expect("HashValue should convert to [u8; 32]");
        let _txn_hash: u128 = txn_hash.iter().fold(0, |mut a, &b| {
            a <<= 8;
            a += b as u128;
            a
        });
        extensions.add(move_table_extension::NativeTableContext::new(
            _txn_hash,
            &state_session,
        ));
        let mut vm_session = self
            .vm
            .new_session_with_extensions(&state_session, extensions);

        let sender = senders.get(0).cloned().unwrap_or(AccountAddress::ZERO);

        let mut cost_strategy =
            GasStatus::new(&self.cost_table, GasUnits::new(gas.max_gas_amount()));

        let result = match script {
            Call::Script { code } => {
                vm_session.execute_script(code, type_args, args, senders, &mut cost_strategy)
            }
            Call::ScriptFunction {
                mod_address,
                mod_name,
                func_name,
            } => vm_session.execute_script_function(
                &ModuleId::new(mod_address, mod_name),
                &func_name,
                type_args,
                args,
                senders,
                &mut cost_strategy,
            ),
        };

        let exec_result = result
            .and_then(|_| {
                Self::charge_global_write_gas_usage(&mut cost_strategy, &mut vm_session, &sender)
            })
            .and_then(|_| vm_session.finish_with_extensions())
            .and_then(|vm_effects| state_session.finish_with_extensions(vm_effects));

        self.handle_vm_result_with_extensions(sender, cost_strategy, gas, exec_result, dry_run)
    }
    fn clear(&self) {
        self.vm.clear();
        self.master_of_coin.clear();
    }
}

impl<S, E, B> StateAccess for Mvm<S, E, B>
where
    S: Storage,
    E: EventHandler,
    B: BalanceAccess,
{
    fn get_module(&self, module_id: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        let module_id = bcs::from_bytes(module_id).map_err(Error::msg)?;
        self.state.get_module(&module_id)
    }

    fn get_module_abi(&self, module_id: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        if let Some(bytecode) = self.get_module(module_id)? {
            Ok(Some(
                bcs::to_bytes(&ModuleAbi::from(
                    CompiledModule::deserialize(&bytecode).map_err(Error::msg)?,
                ))
                .map_err(Error::msg)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn get_resource(&self, address: &AccountAddress, tag: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        let tag = bcs::from_bytes(tag).map_err(Error::msg)?;

        let state_session = self.state.state_session(None, &self.master_of_coin);
        state_session.get_resource(address, &tag)
    }
    fn get_table_entry(&self,handle: u128, key: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        log::warn!("Failed to key================:{:?}", key);
        let state_session = self.state.state_session(None, &self.master_of_coin);
        state_session.resolve_table_entry(&TableHandle(handle), key)
    }
}
