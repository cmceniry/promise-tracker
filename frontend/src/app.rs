use gloo_storage::{LocalStorage, Storage};
use leptos::prelude::*;

use crate::components::{ContractCarder, ContractGrapher};
use crate::follow_sync::FollowSync;
use crate::models::Contract;

const CONTRACTS_STORAGE_KEY: &str = "contracts";
const SIMULATIONS_STORAGE_KEY: &str = "simulations";
const FOLLOWED_DIRS_STORAGE_KEY: &str = "followed_dirs";
const FOLLOW_EXCLUSIONS_STORAGE_KEY: &str = "follow_exclusions";

/// Load a list of server paths from localStorage (empty if absent)
fn load_paths_from_storage(key: &str) -> Vec<String> {
    LocalStorage::get(key).unwrap_or_default()
}

/// Save a list of server paths to localStorage
fn save_paths_to_storage(key: &str, paths: &[String]) {
    if let Err(e) = LocalStorage::set(key, paths) {
        web_sys::console::error_1(&format!("Failed to save {}: {:?}", key, e).into());
    }
}

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

/// Load simulations from localStorage, defaults to ["A", "B"]
fn load_simulations_from_storage() -> Vec<String> {
    LocalStorage::get(SIMULATIONS_STORAGE_KEY)
        .unwrap_or_else(|_| vec!["A".to_string(), "B".to_string()])
}

/// Save simulations to localStorage
fn save_simulations_to_storage(simulations: &[String]) {
    if let Err(e) = LocalStorage::set(SIMULATIONS_STORAGE_KEY, simulations) {
        web_sys::console::error_1(&format!("Failed to save simulations: {:?}", e).into());
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Load initial simulations from localStorage
    let initial_simulations = load_simulations_from_storage();

    // Create simulations state signal
    let (simulations, set_simulations) = signal(initial_simulations);

    // Load initial contracts from localStorage
    let initial_contracts = load_contracts_from_storage();

    // Create contract state signals
    let (contracts, set_contracts) = signal(initial_contracts);

    // Following server files: fetches followed cards now and keeps the event
    // stream open while anything is followed. Cards reach it through context.
    let followed_dirs = RwSignal::new(load_paths_from_storage(FOLLOWED_DIRS_STORAGE_KEY));
    let follow_exclusions =
        RwSignal::new(load_paths_from_storage(FOLLOW_EXCLUSIONS_STORAGE_KEY));
    let follow = FollowSync::start(
        contracts,
        set_contracts,
        simulations,
        followed_dirs,
        follow_exclusions,
    );
    provide_context(follow);

    // Persist followed directories and exclusions whenever they change
    Effect::new(move |_| {
        save_paths_to_storage(FOLLOWED_DIRS_STORAGE_KEY, &followed_dirs.get());
    });
    Effect::new(move |_| {
        save_paths_to_storage(FOLLOW_EXCLUSIONS_STORAGE_KEY, &follow_exclusions.get());
    });

    // Persist simulations to localStorage whenever they change
    Effect::new(move |_| {
        let current_simulations = simulations.get();
        save_simulations_to_storage(&current_simulations);
    });

    // Persist contracts to localStorage whenever they change
    Effect::new(move |_| {
        let current_contracts = contracts.get();
        save_contracts_to_storage(&current_contracts);
    });

    // Add simulation callback
    let add_simulation = move |_| {
        let current_sims = simulations.get();

        // Check maximum constraint (5 simulations: A-E)
        if current_sims.len() >= 5 {
            web_sys::window()
                .unwrap()
                .alert_with_message("Maximum of 5 simulations reached (A-E)")
                .ok();
            return;
        }

        // Generate next letter by incrementing last char
        let next_letter = if current_sims.is_empty() {
            "A".to_string()
        } else {
            let last = current_sims.last().unwrap();
            let last_char = last.chars().next().unwrap();
            let next_char = ((last_char as u8) + 1) as char;
            next_char.to_string()
        };

        set_simulations.update(|sims| {
            sims.push(next_letter);
        });
    };

    // Remove simulation callback
    let remove_simulation = move |_| {
        let current_sims = simulations.get();

        // Check minimum constraint (must keep at least 1 simulation)
        if current_sims.len() <= 1 {
            web_sys::window()
                .unwrap()
                .alert_with_message("Must keep at least 1 simulation (A)")
                .ok();
            return;
        }

        // Get the simulation being removed
        let removed_sim = current_sims.last().unwrap().clone();

        // Remove last simulation from the list
        set_simulations.update(|sims| {
            sims.pop();
        });

        // Clean up all contracts: remove deleted simulation from their sims HashSet
        set_contracts.update(|contracts| {
            for contract in contracts.iter_mut() {
                contract.sims.remove(&removed_sim);
            }
        });
    };

    view! {
        <div class="App">
            <div class="container-fluid">
                <div class="row app-row">
                    <div class="col-md-3 app-column">
                        <ContractCarder
                            contracts=contracts
                            set_contracts=set_contracts
                            simulations=simulations
                        />
                    </div>
                    <div class="col-md-9 app-column">
                        <ContractGrapher
                            contracts=contracts
                            simulations=simulations
                            ready=follow.ready
                            on_add_simulation=Callback::new(add_simulation)
                            on_remove_simulation=Callback::new(remove_simulation)
                        />
                    </div>
                </div>
            </div>
        </div>
    }
}
