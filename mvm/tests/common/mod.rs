#![allow(dead_code)]

use diem_types::account_config::CORE_CODE_ADDRESS;
use move_core_types::{
    identifier::Identifier, language_storage::ModuleId, resolver::ModuleResolver,
};
use mvm::{genesis::init_storage, io::state::State, mvm::Mvm};

use crate::common::mock::{BankMock, EventHandlerMock, StorageMock};

pub mod assets;
pub mod mock;

pub fn vm() -> (
    Mvm<StorageMock, EventHandlerMock, BankMock>,
    StorageMock,
    EventHandlerMock,
    BankMock,
) {
    let store = StorageMock::new();
    let event = EventHandlerMock::default();
    let bank = BankMock::default();
    init_storage(store.clone(), Default::default()).unwrap();

    let vm = Mvm::new(store.clone(), event.clone(), bank.clone()).unwrap();
    (vm, store, event, bank)
}

pub fn contains_core_module(state: &State<StorageMock>, name: &str) {
    if state
        .get_module(&ModuleId::new(
            CORE_CODE_ADDRESS,
            Identifier::new(name).unwrap(),
        ))
        .unwrap()
        .is_none()
    {
        panic!("Module {} not found", name);
    }
}
