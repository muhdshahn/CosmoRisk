// CosmoRisk - NEO Tracking & Deflection Simulator
// Main entry point for Tauri application

mod api_client;
mod physics_engine;
mod state_manager;

use state_manager::{
    apply_deflection, apply_gravity_tractor, apply_ion_beam, fetch_asteroid_by_id, fetch_asteroids,
    fetch_asteroids_by_date, fetch_more_asteroids, get_body_details, get_cached_asteroids,
    get_impact_prediction, get_simulation_state, reset_simulation, run_monte_carlo, set_api_key,
    set_paused, set_time_scale, set_time_step, start_simulation_loop, AppState,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new();

    // Start background simulation loop
    let sim_state = app_state.simulation.clone();
    let is_running = app_state.is_running.clone();
    *is_running.write() = true;
    start_simulation_loop(sim_state, is_running);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_simulation_state,
            set_paused,
            set_time_scale,
            set_time_step,
            reset_simulation,
            apply_deflection,
            set_api_key,
            fetch_asteroids,
            fetch_more_asteroids,
            get_body_details,
            get_cached_asteroids,
            apply_ion_beam,
            get_impact_prediction,
            run_monte_carlo,
            apply_gravity_tractor,
            fetch_asteroids_by_date,
            fetch_asteroid_by_id,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
