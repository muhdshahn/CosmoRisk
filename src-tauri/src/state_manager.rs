// State Manager - Thread-safe simulation state handling
// Manages the simulation state and provides Tauri commands for frontend

use parking_lot::RwLock;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::api_client::{CacheManager, NeoWsClient, ProcessedAsteroid};
use crate::physics_engine::{
    calculate_total_energy, SimulationState, Vector3, VelocityVerletIntegrator,
};

// =============================================================================
// GLOBAL STATE
// =============================================================================

pub struct AppState {
    pub simulation: Arc<RwLock<SimulationState>>,
    pub cache: Arc<CacheManager>,
    pub api_key: Arc<RwLock<Option<String>>>,
    pub is_running: Arc<RwLock<bool>>,
}

impl AppState {
    pub fn new() -> Self {
        // Initialize with J2000 epoch (2000-01-01 12:00 TT)
        let julian_date = 2451545.0;

        Self {
            simulation: Arc::new(RwLock::new(SimulationState::new(julian_date))),
            cache: Arc::new(CacheManager::new()),
            api_key: Arc::new(RwLock::new(None)),
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn set_api_key(&self, key: String) {
        *self.api_key.write() = Some(key);
    }

    pub fn get_api_key(&self) -> Option<String> {
        self.api_key.read().clone()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// SIMULATION LOOP (runs in background thread)
// =============================================================================

pub fn start_simulation_loop(state: Arc<RwLock<SimulationState>>, is_running: Arc<RwLock<bool>>) {
    thread::spawn(move || {
        let _integrator = VelocityVerletIntegrator::new(3600.0); // Reserved for future optimization
        let target_frame_time = Duration::from_millis(16); // ~60 FPS

        loop {
            let start = Instant::now();

            // Check if simulation should run
            let (should_run, time_scale, dt) = {
                let sim = state.read();
                (!sim.is_paused, sim.time_scale, sim.dt)
            };

            if should_run {
                let mut sim = state.write();

                // Number of physics steps per frame based on time scale
                let steps = ((time_scale / 60.0) as usize).max(1).min(100);
                let sun_pos = Vector3::zero(); // Sun at origin in heliocentric

                for _ in 0..steps {
                    // Step physics
                    let mut integrator_step = VelocityVerletIntegrator::new(dt);
                    integrator_step.enable_j2 = true;
                    integrator_step.enable_srp = true;
                    integrator_step.step(&mut sim.bodies, &sun_pos);

                    sim.time += dt;
                    sim.julian_date += dt / 86400.0; // Convert seconds to days
                }

                // Update energy for drift monitoring
                sim.total_energy = calculate_total_energy(&sim.bodies);
            }

            // Check if we should stop
            if !*is_running.read() {
                break;
            }

            // Sleep to maintain frame rate
            let elapsed = start.elapsed();
            if elapsed < target_frame_time {
                thread::sleep(target_frame_time - elapsed);
            }
        }
    });
}

// =============================================================================
// SERIALIZABLE STATE FOR FRONTEND
// =============================================================================

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendBody {
    pub id: String,
    pub name: String,
    pub body_type: String,
    pub position: [f64; 3], // AU (scaled for frontend)
    pub velocity: [f64; 3], // km/s
    pub radius: f64,        // km
    pub is_hazardous: bool,
    pub semi_major_axis_au: f64,           // For orbit visualization
    pub eccentricity: f64,                 // For elliptical orbits
    pub inclination_rad: f64,              // Orbital inclination (radians)
    pub longitude_ascending_node_rad: f64, // RAAN Ω (radians)
    pub argument_perihelion_rad: f64,      // Argument of perihelion ω (radians)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendState {
    pub bodies: Vec<FrontendBody>,
    pub time: f64,
    pub julian_date: f64,
    pub is_paused: bool,
    pub time_scale: f64,
    pub energy_drift: f64,
    pub asteroid_count: usize,
}

impl SimulationState {
    pub fn to_frontend(&self) -> FrontendState {
        use crate::physics_engine::AU;

        let bodies: Vec<FrontendBody> = self
            .bodies
            .iter()
            .map(|b| {
                // Extract orbital elements if available (for asteroids)
                let (a_au, e, i, raan, omega) = if let Some(ref oe) = b.orbital_elements {
                    (
                        oe.semi_major_axis / AU, // Convert to AU
                        oe.eccentricity,
                        oe.inclination,
                        oe.longitude_ascending_node,
                        oe.argument_perihelion,
                    )
                } else {
                    (1.0, 0.0, 0.0, 0.0, 0.0) // Defaults for planets/sun
                };

                FrontendBody {
                    id: b.id.clone(),
                    name: b.name.clone(),
                    body_type: format!("{:?}", b.body_type),
                    position: [
                        b.state.position.x / AU, // Convert to AU
                        b.state.position.y / AU,
                        b.state.position.z / AU,
                    ],
                    velocity: [
                        b.state.velocity.x / 1000.0, // Convert to km/s
                        b.state.velocity.y / 1000.0,
                        b.state.velocity.z / 1000.0,
                    ],
                    radius: b.radius / 1000.0, // Convert to km
                    is_hazardous: false,       // Set from asteroid data
                    semi_major_axis_au: a_au,
                    eccentricity: e,
                    inclination_rad: i,
                    longitude_ascending_node_rad: raan,
                    argument_perihelion_rad: omega,
                }
            })
            .collect();

        let asteroid_count = self
            .bodies
            .iter()
            .filter(|b| matches!(b.body_type, crate::physics_engine::BodyType::Asteroid))
            .count();

        let energy_drift = if self.initial_energy.abs() > 1e-20 {
            (self.total_energy - self.initial_energy).abs() / self.initial_energy.abs()
        } else {
            0.0
        };

        FrontendState {
            bodies,
            time: self.time,
            julian_date: self.julian_date,
            is_paused: self.is_paused,
            time_scale: self.time_scale,
            energy_drift,
            asteroid_count,
        }
    }
}

// =============================================================================
// TAURI COMMANDS
// =============================================================================

use tauri::State;

#[tauri::command]
pub fn get_simulation_state(state: State<AppState>) -> FrontendState {
    state.simulation.read().to_frontend()
}

#[tauri::command]
pub fn set_paused(state: State<AppState>, paused: bool) {
    state.simulation.write().is_paused = paused;
}

#[tauri::command]
pub fn set_time_scale(state: State<AppState>, scale: f64) {
    let clamped = scale.max(1.0).min(1_000_000.0);
    state.simulation.write().time_scale = clamped;
}

#[tauri::command]
pub fn set_time_step(state: State<AppState>, dt: f64) {
    let clamped = dt.max(1.0).min(86400.0);
    state.simulation.write().dt = clamped;
}

#[tauri::command]
pub fn reset_simulation(state: State<AppState>) {
    let julian_date = 2451545.0;
    *state.simulation.write() = SimulationState::new(julian_date);
}

#[tauri::command]
pub fn apply_deflection(
    state: State<AppState>,
    body_id: String,
    delta_v: [f64; 3],
) -> Result<(), String> {
    let dv = Vector3::new(delta_v[0], delta_v[1], delta_v[2]);
    state.simulation.write().apply_impulse(&body_id, dv);
    Ok(())
}

#[tauri::command]
pub fn set_api_key(state: State<AppState>, api_key: String) {
    state.set_api_key(api_key);
}

#[tauri::command]
pub async fn fetch_asteroids(state: State<'_, AppState>) -> Result<usize, String> {
    let api_key = state.get_api_key().ok_or("API key not set")?;

    let client = NeoWsClient::new(api_key);

    // Fetch NEOs from browse API (first page with 20 items)
    let (asteroids, _total_pages) = client.browse(0, 20).await?;

    // Add asteroids to simulation
    {
        let mut sim = state.simulation.write();
        for asteroid in &asteroids {
            sim.add_asteroid(
                &asteroid.id,
                &asteroid.name,
                asteroid.orbital_elements.clone(),
                asteroid.estimated_diameter_m,
            );
        }
    }

    // Cache the data
    state.cache.set_asteroids(asteroids.clone());

    Ok(asteroids.len())
}

#[tauri::command]
pub async fn fetch_more_asteroids(
    state: State<'_, AppState>,
    page: i32,
    size: i32,
) -> Result<usize, String> {
    let api_key = state.get_api_key().ok_or("API key not set")?;

    let client = NeoWsClient::new(api_key);
    let (asteroids, _) = client.browse(page, size.min(20)).await?;

    {
        let mut sim = state.simulation.write();
        for asteroid in &asteroids {
            // Check if not already added
            if !sim.bodies.iter().any(|b| b.id == asteroid.id) {
                sim.add_asteroid(
                    &asteroid.id,
                    &asteroid.name,
                    asteroid.orbital_elements.clone(),
                    asteroid.estimated_diameter_m,
                );
            }
        }
    }

    Ok(asteroids.len())
}

#[tauri::command]
pub fn get_body_details(state: State<AppState>, body_id: String) -> Option<FrontendBody> {
    let sim = state.simulation.read();
    sim.bodies.iter().find(|b| b.id == body_id).map(|b| {
        use crate::physics_engine::AU;

        // Extract orbital elements if available
        let (a_au, e, i, raan, omega) = if let Some(ref oe) = b.orbital_elements {
            (
                oe.semi_major_axis / AU,
                oe.eccentricity,
                oe.inclination,
                oe.longitude_ascending_node,
                oe.argument_perihelion,
            )
        } else {
            (1.0, 0.0, 0.0, 0.0, 0.0)
        };

        FrontendBody {
            id: b.id.clone(),
            name: b.name.clone(),
            body_type: format!("{:?}", b.body_type),
            position: [
                b.state.position.x / AU,
                b.state.position.y / AU,
                b.state.position.z / AU,
            ],
            velocity: [
                b.state.velocity.x / 1000.0,
                b.state.velocity.y / 1000.0,
                b.state.velocity.z / 1000.0,
            ],
            radius: b.radius / 1000.0,
            is_hazardous: false,
            semi_major_axis_au: a_au,
            eccentricity: e,
            inclination_rad: i,
            longitude_ascending_node_rad: raan,
            argument_perihelion_rad: omega,
        }
    })
}

#[tauri::command]
pub fn get_cached_asteroids(state: State<AppState>) -> Vec<ProcessedAsteroid> {
    state.cache.get_asteroids()
}

/// Ion beam deflection - applies continuous low thrust
#[tauri::command]
pub fn apply_ion_beam(
    state: State<AppState>,
    body_id: String,
    thrust_direction: [f64; 3],
    thrust_magnitude: f64,
    duration: f64,
) -> Result<(), String> {
    let direction = Vector3::new(
        thrust_direction[0],
        thrust_direction[1],
        thrust_direction[2],
    );
    state
        .simulation
        .write()
        .apply_ion_beam(&body_id, direction, thrust_magnitude, duration);
    Ok(())
}

/// Get impact prediction for an asteroid
#[derive(serde::Serialize)]
pub struct ImpactPrediction {
    pub min_distance_km: f64,
    pub time_to_closest_days: f64,
    pub is_hazardous: bool,
}

#[tauri::command]
pub fn get_impact_prediction(state: State<AppState>, body_id: String) -> Option<ImpactPrediction> {
    let sim = state.simulation.read();
    let (min_dist, time_days) = sim.calculate_earth_approach(&body_id)?;

    // Hazardous if closer than 7.5 million km (0.05 AU)
    let is_hazardous = min_dist < 7_500_000.0;

    Some(ImpactPrediction {
        min_distance_km: min_dist,
        time_to_closest_days: time_days,
        is_hazardous,
    })
}

// =============================================================================
// MONTE CARLO IMPACT ANALYSIS
// =============================================================================

use crate::physics_engine::{monte_carlo_impact_probability, R_EARTH};

#[derive(serde::Serialize)]
pub struct MonteCarloResultFrontend {
    pub num_runs: u32,
    pub num_impacts: u32,
    pub impact_probability: f64,
    pub mean_moid_km: f64,
    pub std_moid_km: f64,
    pub min_moid_km: f64,
    pub palermo_scale: f64,
}

/// Run Monte Carlo impact probability analysis
#[tauri::command]
pub fn run_monte_carlo(
    state: State<AppState>,
    body_id: String,
    position_uncertainty_km: f64,
    velocity_uncertainty_ms: f64,
    num_runs: u32,
    simulation_days: f64,
) -> Result<MonteCarloResultFrontend, String> {
    let sim = state.simulation.read();

    let body = sim
        .bodies
        .iter()
        .find(|b| b.id == body_id)
        .ok_or("Asteroid not found")?;

    // Convert uncertainties to meters
    let pos_uncertainty = position_uncertainty_km * 1000.0;
    let vel_uncertainty = velocity_uncertainty_ms;

    let result = monte_carlo_impact_probability(
        body,
        pos_uncertainty,
        vel_uncertainty,
        num_runs.clamp(100, 10000),
        simulation_days.clamp(1.0, 3650.0),
        R_EARTH,
    );

    Ok(MonteCarloResultFrontend {
        num_runs: result.num_runs,
        num_impacts: result.num_impacts,
        impact_probability: result.impact_probability,
        mean_moid_km: result.mean_moid,
        std_moid_km: result.std_moid,
        min_moid_km: result.min_moid,
        palermo_scale: result.palermo_scale,
    })
}

// =============================================================================
// GRAVITY TRACTOR DEFLECTION
// =============================================================================

/// Apply gravity tractor deflection over time
#[tauri::command]
pub fn apply_gravity_tractor(
    state: State<AppState>,
    body_id: String,
    spacecraft_mass_kg: f64,
    hover_distance_m: f64,
    duration_days: f64,
) -> Result<GravityTractorResult, String> {
    let mut sim = state.simulation.write();

    let body = sim
        .bodies
        .iter()
        .find(|b| b.id == body_id)
        .ok_or("Asteroid not found")?
        .clone();

    let integrator = VelocityVerletIntegrator::new(sim.dt);
    let (accel, deflection_time) = integrator.calculate_gravity_tractor(
        &body,
        spacecraft_mass_kg.clamp(500.0, 50000.0),
        hover_distance_m.clamp(50.0, 500.0),
        0.0, // Lead angle
    );

    // Apply the acceleration as delta-v over duration
    let duration_seconds = duration_days * 86400.0;
    let delta_v = accel.scale(duration_seconds);

    if let Some(body) = sim.bodies.iter_mut().find(|b| b.id == body_id) {
        body.state.velocity = body.state.velocity.add(&delta_v);
    }

    Ok(GravityTractorResult {
        delta_v_applied_ms: delta_v.magnitude(),
        estimated_deflection_time_days: deflection_time,
    })
}

#[derive(serde::Serialize)]
pub struct GravityTractorResult {
    pub delta_v_applied_ms: f64,
    pub estimated_deflection_time_days: f64,
}

// =============================================================================
// DATE-BASED ASTEROID SEARCH
// =============================================================================

/// Fetch asteroids by close approach date range
#[tauri::command]
pub async fn fetch_asteroids_by_date(
    state: State<'_, AppState>,
    start_date: String,
    end_date: String,
) -> Result<usize, String> {
    let api_key = state.get_api_key().ok_or("API key not set")?;

    let client = NeoWsClient::new(api_key);
    let asteroids = client.fetch_feed(&start_date, &end_date).await?;

    {
        let mut sim = state.simulation.write();
        for asteroid in &asteroids {
            if !sim.bodies.iter().any(|b| b.id == asteroid.id) {
                sim.add_asteroid(
                    &asteroid.id,
                    &asteroid.name,
                    asteroid.orbital_elements.clone(),
                    asteroid.estimated_diameter_m,
                );
            }
        }
    }

    state.cache.set_asteroids(asteroids.clone());
    Ok(asteroids.len())
}

// =============================================================================
// SINGLE ASTEROID LOOKUP
// =============================================================================

/// Fetch a single asteroid by NASA NEO ID
#[tauri::command]
pub async fn fetch_asteroid_by_id(
    state: State<'_, AppState>,
    neo_id: String,
) -> Result<ProcessedAsteroid, String> {
    let api_key = state.get_api_key().ok_or("API key not set")?;

    let client = NeoWsClient::new(api_key);
    let asteroid = client.fetch_neo(&neo_id).await?;

    {
        let mut sim = state.simulation.write();
        if !sim.bodies.iter().any(|b| b.id == asteroid.id) {
            sim.add_asteroid(
                &asteroid.id,
                &asteroid.name,
                asteroid.orbital_elements.clone(),
                asteroid.estimated_diameter_m,
            );
        }
    }

    Ok(asteroid)
}
