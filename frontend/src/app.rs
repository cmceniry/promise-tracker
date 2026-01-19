use gloo_storage::{LocalStorage, Storage};
use leptos::prelude::*;

use crate::components::{ContractCarder, ContractGrapher};
use crate::models::Contract;

const CONTRACTS_STORAGE_KEY: &str = "contracts";

/// Load contracts from localStorage
fn load_contracts_from_storage() -> Vec<Contract> {
    LocalStorage::get(CONTRACTS_STORAGE_KEY).unwrap_or_default()
}

/// Save contracts to localStorage
fn save_contracts_to_storage(contracts: &[Contract]) {
    if let Err(e) = LocalStorage::set(CONTRACTS_STORAGE_KEY, contracts) {
        web_sys::console::error_1(&format!("Failed to save contracts: {:?}", e).into());
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Define simulations
    let simulations = vec!["A".to_string(), "B".to_string()];

    // Load initial contracts from localStorage
    let initial_contracts = load_contracts_from_storage();

    // Create contract state signals
    let (contracts, set_contracts) = signal(initial_contracts);

    // Persist contracts to localStorage whenever they change
    Effect::new(move |_| {
        let current_contracts = contracts.get();
        save_contracts_to_storage(&current_contracts);
    });

    view! {
        <div class="App">
            <div class="container-fluid">
                <div class="row">
                    <div class="col-md-3" style="overflow-y: scroll;">
                        <ContractCarder
                            contracts=contracts
                            set_contracts=set_contracts
                            simulations=simulations.clone()
                        />
                    </div>
                    <div class="col-md-9" style="overflow-y: scroll;">
                        <ContractGrapher
                            contracts=contracts
                            simulations=simulations.clone()
                        />
                    </div>
                </div>
            </div>
        </div>
    }
}
