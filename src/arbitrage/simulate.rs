use std::collections::HashMap;

use log::error;
use log::info;
use rust_socketio::asynchronous::Client;

use super::types::{SwapPath, SwapRouteSimulation, TokenInfos};
use crate::markets::meteora::simulate_route_meteora;
use crate::markets::{
    orca_whirpools::simulate_route_orca_whirpools,
    raydium::simulate_route_raydium,
    types::{DexLabel, Market},
};

pub async fn simulate_path(
    simulation_amount: u64,
    path: SwapPath,
    markets: Vec<Market>,
    tokens_infos: HashMap<String, TokenInfos>,
    mut route_simulation: HashMap<Vec<u32>, Vec<SwapRouteSimulation>>,
) -> (
    HashMap<Vec<u32>, Vec<SwapRouteSimulation>>,
    Vec<SwapRouteSimulation>,
    f64,
) {
    println!("🚕🚕🚕🚕  NEW PATH  🚕🚕🚕🚕");
    println!("Nb. Hops : {}", path.hops);
    let decimals = 9;
    let mut amount_in = simulation_amount;
    let amount_begin = amount_in;

    let mut swap_simulation_result: Vec<SwapRouteSimulation> = Vec::new();

    for (i, route) in path.paths.iter().enumerate() {
        let market: Option<Market> = markets
            .iter()
            .find(|&market| market.id == route.pool_address)
            .cloned();

        match path.hops {
            1 => {
                if i == 0 && route_simulation.contains_key(&vec![path.id_paths[i]]) {
                    let swap_sim = route_simulation.get(&vec![path.id_paths[i]]).unwrap();
                    amount_in = swap_sim[0]
                        .estimated_amount_out
                        .as_str()
                        .parse()
                        .expect("Bad conversion String to f64");
                    println!("📌 NO SIMULATION Route Id: {}", swap_sim[0].id_route);
                    swap_simulation_result.push(swap_sim[0].clone());
                    continue;
                }
            }
            2 => {
                if i == 0 && route_simulation.contains_key(&vec![path.id_paths[i]]) {
                    let swap_sim = route_simulation.get(&vec![path.id_paths[i]]).unwrap();
                    amount_in = swap_sim[0]
                        .estimated_amount_out
                        .as_str()
                        .parse()
                        .expect("Bad conversion String to f64");
                    println!("📌 NO SIMULATION Route 1 Id: {}", swap_sim[0].id_route);
                    swap_simulation_result.push(swap_sim[0].clone());
                    continue;
                }
                if i == 1 && route_simulation.contains_key(&vec![path.id_paths[i - 1], path.id_paths[i]]) {
                    let swap_sim = route_simulation
                        .get(&vec![path.id_paths[i - 1], path.id_paths[i]])
                        .unwrap();
                    amount_in = swap_sim[1]
                        .estimated_amount_out
                        .as_str()
                        .parse()
                        .expect("Bad conversion String to f64");
                    println!("📌 NO SIMULATION Route 2 Id: {}", swap_sim[1].id_route);
                    swap_simulation_result.push(swap_sim[1].clone());
                    continue;
                }
            }
            _ => {
                println!("⛔ Invalid number of hops")
            } //...
        }
        match route.dex {
            DexLabel::Orca => {
                println!(" ⚠️⚠️ ONE ORCA POOL ");
            }
            DexLabel::OrcaWhirlpools => {
                println!("🏊 ORCA_WHIRLPOOLS - POOL");
                println!("Address: {:?}", route.pool_address);
                match simulate_route_orca_whirpools(
                    true,
                    amount_in,
                    route.clone(),
                    market.unwrap(),
                    tokens_infos.clone(),
                )
                .await
                {
                    Ok(value) => {
                        let (amount_out_u64, min_amount_out_u64) = value; // Values are now u64
                        // println!("Amount out: {}", amount_out_u64);

                        let swap_sim: SwapRouteSimulation = SwapRouteSimulation {
                            id_route: route.id,
                            pool_address: route.pool_address.clone(),
                            dex_label: DexLabel::OrcaWhirlpools,
                            token_0to1: route.token_0to1,
                            token_in: route.token_in.clone(),
                            token_out: route.token_out.clone(),
                            amount_in,
                            estimated_amount_out: amount_out_u64.to_string(), // Convert u64 to String
                            minimum_amount_out: min_amount_out_u64, // Already u64
                        };

                        //1rst route
                        if i == 0 && !route_simulation.contains_key(&vec![path.id_paths[i]]) {
                            route_simulation.insert(vec![route.id], vec![swap_sim.clone()]);
                        }

                        //2nd route
                        if i == 1
                            && path.hops == 2
                            && !route_simulation
                                .contains_key(&vec![path.id_paths[i - 1], path.id_paths[i]])
                        {
                            let swap_sim_prev_route =
                                route_simulation.get(&vec![path.id_paths[i - 1]]).unwrap();
                            route_simulation.insert(
                                vec![path.id_paths[i - 1], path.id_paths[i]],
                                vec![swap_sim_prev_route[0].clone(), swap_sim.clone()],
                            );
                        }

                        swap_simulation_result.push(swap_sim.clone());
                        amount_in = amount_out_u64; // amount_in is u64, amount_out_u64 is u64
                    }
                    Err(e) => { // Changed error handling
                        error!(
                            "❌ SIMULATION ERROR for route: {:?}, ORCA_WHIRLPOOLS POOL, Address: {:?}, ERROR: {:?}",
                            path.id_paths, route.pool_address, e
                        );
                        println!("🔚 Skipped Path due to Orca Whirlpools simulation error");
                        let empty_result: Vec<SwapRouteSimulation> = Vec::new();
                        return (route_simulation, empty_result, 0.0);
                    }
                }
            }
            DexLabel::Raydium => {
                println!("🏊 RAYDIUM - POOL");
                println!("Address: {:?}", route.pool_address);
                match simulate_route_raydium(
                    true,
                    amount_in,
                    route.clone(),
                    market.unwrap(),
                    tokens_infos.clone(),
                )
                .await
                {
                    Ok(value) => {
                        let (amount_out_u64, min_amount_out_u64) = value; // Values are now u64
                        // println!("Amount out: {}", amount_out_u64);

                        let swap_sim: SwapRouteSimulation = SwapRouteSimulation {
                            id_route: route.id, 
                            pool_address: route.pool_address.clone(),
                            dex_label: DexLabel::Raydium, // Corrected: Should be Raydium
                            token_0to1: route.token_0to1,
                            token_in: route.token_in.clone(),
                            token_out: route.token_out.clone(),
                            amount_in,
                            estimated_amount_out: amount_out_u64.to_string(), // Convert u64 to String
                            minimum_amount_out: min_amount_out_u64, // Already u64
                        };

                        //1rst route
                        if i == 0 && !route_simulation.contains_key(&vec![path.id_paths[i]]) {
                            route_simulation.insert(vec![route.id], vec![swap_sim.clone()]);
                        }
                        //2nd route
                        if i == 1
                            && path.hops == 2
                            && !route_simulation
                                .contains_key(&vec![path.id_paths[i - 1], path.id_paths[i]])
                        {
                            let swap_sim_prev_route =
                                route_simulation.get(&vec![path.id_paths[i - 1]]).unwrap();
                            route_simulation.insert(
                                vec![path.id_paths[i - 1], path.id_paths[i]],
                                vec![swap_sim_prev_route[0].clone(), swap_sim.clone()],
                            );
                        }

                        swap_simulation_result.push(swap_sim.clone());
                        amount_in = amount_out_u64; // amount_in is u64, amount_out_u64 is u64
                    }
                    Err(e) => { // Changed error handling
                        error!(
                            "❌ SIMULATION ERROR for route: {:?}, RAYDIUM POOL, Address: {:?}, ERROR: {:?}",
                            path.id_paths, route.pool_address, e
                        );
                        println!("🔚 Skipped Path due to Raydium simulation error");
                        let empty_result: Vec<SwapRouteSimulation> = Vec::new();
                        return (route_simulation, empty_result, 0.0);
                    }
                }
            }
            DexLabel::RaydiumClmm => {
                println!(" ⚠️⚠️ ONE RAYDIUM_CLMM POOL ");
            }
            DexLabel::Meteora => {
                // println!(" ⚠️⚠️ ONE METEORA POOL ");
                println!("🏊 METEORA - POOL");
                println!("Address: {:?}", route.pool_address);
                match simulate_route_meteora(
                    true,
                    amount_in,
                    route.clone(),
                    market.unwrap(),
                    tokens_infos.clone(),
                )
                .await
                {
                    Ok(value) => {
                        let (amount_out_u64, min_amount_out_u64) = value; // Values are now u64
                        // println!("Amount out: {}", amount_out_u64);

                        let swap_sim: SwapRouteSimulation = SwapRouteSimulation {
                            id_route: route.id,
                            pool_address: route.pool_address.clone(),
                            dex_label: DexLabel::Meteora,
                            token_0to1: route.token_0to1,
                            token_in: route.token_in.clone(),
                            token_out: route.token_out.clone(),
                            amount_in,
                            estimated_amount_out: amount_out_u64.to_string(), // Convert u64 to String
                            minimum_amount_out: min_amount_out_u64, // Already u64
                        };

                        //1rst route
                        if i == 0 && !route_simulation.contains_key(&vec![path.id_paths[i]]) {
                            route_simulation.insert(vec![route.id], vec![swap_sim.clone()]);
                        }
                        //2nd route
                        if i == 1
                            && path.hops == 2
                            && !route_simulation
                                .contains_key(&vec![path.id_paths[i - 1], path.id_paths[i]])
                        {
                            let swap_sim_prev_route =
                                route_simulation.get(&vec![path.id_paths[i - 1]]).unwrap();
                            route_simulation.insert(
                                vec![path.id_paths[i - 1], path.id_paths[i]],
                                vec![swap_sim_prev_route[0].clone(), swap_sim.clone()],
                            );
                        }

                        swap_simulation_result.push(swap_sim.clone());
                        amount_in = amount_out_u64; // amount_in is u64, amount_out_u64 is u64
                    }
                    Err(e) => { // Changed error handling
                        error!(
                            "❌ SIMULATION ERROR for route: {:?}, METEORA POOL, Address: {:?}, ERROR: {:?}",
                            path.id_paths, route.pool_address, e
                        );
                        println!("🔚 Skipped Path due to Meteora simulation error");
                        let empty_result: Vec<SwapRouteSimulation> = Vec::new();
                        return (route_simulation, empty_result, 0.0);
                    }
                }
            }
        }
    }
    info!(
        "💵💵 Simulation of Swap Path [Id: {:?}] // Amount In: {} {} // Amount Out: {} {}",
        path.id_paths,
        amount_begin as f64 / 10_f64.powf(decimals as f64),
        "SOL",
        amount_in as f64 / 10_f64.powf(decimals as f64),
        "SOL"
    );

    //If interesting path
    let difference = amount_in as f64 - amount_begin as f64;
    if difference > 0.0 {
        info!(
            "💸💸💸💸💸💸💸💸💸💸 Path simulate {} {} positive difference",
            difference / 10_f64.powf(decimals as f64),
            "SOL"
        );
    }

    return (route_simulation, swap_simulation_result, difference);
}

pub async fn simulate_path_precision(
    amount_input: u64,
    _socket: Client,
    path: SwapPath,
    markets: Vec<Market>,
    tokens_infos: HashMap<String, TokenInfos>,
) -> (Vec<SwapRouteSimulation>, f64) {
    // println!("🚕🚕🚕🚕     NEW PRECISION PATH    🚕🚕🚕🚕");
    // println!("Nb. Hops : {}", path.hops);

    let decimals: u32 = 9;
    let amount_begin = amount_input;
    let mut amount_in = amount_input;

    let mut swap_simulation_result: Vec<SwapRouteSimulation> = Vec::new();

    for route in path.paths.iter() {
        let market: Option<Market> = markets
            .iter()
            .find(|&market| market.id == route.pool_address)
            .cloned();

        match route.dex {
            DexLabel::Orca | DexLabel::RaydiumClmm => { // Merged Orca and RaydiumClmm
                // println!(" ⚠️⚠️ ONE ORCA POOL / RAYDIUM_CLMM POOL ");
            }
            DexLabel::OrcaWhirlpools => {
                // println!("ORCA_WHIRLPOOLS - POOL");
                // println!("Address: {:?}", route.pool_address);
                match simulate_route_orca_whirpools(
                    false,
                    amount_in,
                    route.clone(),
                    market.unwrap(),
                    tokens_infos.clone(),
                )
                .await
                {
                    Ok(value) => {
                        let (amount_out_u64, min_amount_out_u64) = value; // Values are now u64
                        // println!("Amount out: {}", amount_out_u64);

                        let swap_sim: SwapRouteSimulation = SwapRouteSimulation {
                            id_route: route.id,
                            pool_address: route.pool_address.clone(),
                            dex_label: DexLabel::OrcaWhirlpools,
                            token_0to1: route.token_0to1,
                            token_in: route.token_in.clone(),
                            token_out: route.token_out.clone(),
                            amount_in,
                            estimated_amount_out: amount_out_u64.to_string(), // Convert u64 to String
                            minimum_amount_out: min_amount_out_u64, // Already u64
                        };

                        swap_simulation_result.push(swap_sim.clone());
                        amount_in = amount_out_u64; // amount_in is u64
                    }
                    Err(e) => { // Changed error handling
                        error!(
                            "❌ PRECISION SIMULATION ERROR for route: {:?}, ORCA_WHIRLPOOLS POOL, Address: {:?}, ERROR: {:?}",
                            path.id_paths, route.pool_address, e
                        );
                        let empty_result: Vec<SwapRouteSimulation> = Vec::new();
                        return (empty_result, 0.0);
                    }
                }
            }
            DexLabel::Raydium => {
                // println!("RAYDIUM - POOL");
                // println!("Address: {:?}", route.pool_address);
                match simulate_route_raydium(
                    false,
                    amount_in,
                    route.clone(),
                    market.unwrap(),
                    tokens_infos.clone(),
                )
                .await
                {
                    Ok(value) => {
                        let (amount_out_u64, min_amount_out_u64) = value; // Values are now u64
                        // println!("Amount out: {}", amount_out_u64);

                        let swap_sim: SwapRouteSimulation = SwapRouteSimulation {
                            id_route: route.id,
                            pool_address: route.pool_address.clone(),
                            dex_label: DexLabel::Raydium,
                            token_0to1: route.token_0to1,
                            token_in: route.token_in.clone(),
                            token_out: route.token_out.clone(),
                            amount_in,
                            estimated_amount_out: amount_out_u64.to_string(), // Convert u64 to String
                            minimum_amount_out: min_amount_out_u64, // Already u64
                        };

                        swap_simulation_result.push(swap_sim.clone());
                        amount_in = amount_out_u64; // amount_in is u64
                    }
                    Err(e) => { // Changed error handling
                        error!(
                            "❌ PRECISION SIMULATION ERROR for route: {:?}, RAYDIUM POOL, Address: {:?}, ERROR: {:?}",
                            path.id_paths, route.pool_address, e
                        );
                        println!("🔚 Skipped Path due to Raydium precision simulation error");
                        let empty_result: Vec<SwapRouteSimulation> = Vec::new();
                        return (empty_result, 0.0);
                    }
                }
            }
            // DexLabel::RaydiumClmm arm removed due to merge with DexLabel::Orca
            DexLabel::Meteora => {
                // println!(" ⚠️⚠️ ONE METEORA POOL ");
                // println!("METEORA - POOL");
                // println!("Address: {:?}", route.pool_address);
                match simulate_route_meteora(
                    false,
                    amount_in,
                    route.clone(),
                    market.unwrap(),
                    tokens_infos.clone(),
                )
                .await
                {
                    Ok(value) => {
                        let (amount_out_u64, min_amount_out_u64) = value; // Values are now u64
                        // println!("Amount out: {}", amount_out_u64);

                        let swap_sim: SwapRouteSimulation = SwapRouteSimulation {
                            id_route: route.id, // .clone() removed as route.id is u32 (Copy)
                            pool_address: route.pool_address.clone(),
                            dex_label: DexLabel::Meteora, // Corrected: Should be Meteora
                            token_0to1: route.token_0to1,
                            token_in: route.token_in.clone(),
                            token_out: route.token_out.clone(),
                            amount_in,
                            estimated_amount_out: amount_out_u64.to_string(), // Convert u64 to String
                            minimum_amount_out: min_amount_out_u64, // Already u64
                        };

                        swap_simulation_result.push(swap_sim.clone());
                        amount_in = amount_out_u64; // amount_in is u64
                    }
                    Err(e) => { // Changed error handling
                        error!(
                            "❌ PRECISION SIMULATION ERROR for route: {:?}, METEORA POOL, Address: {:?}, ERROR: {:?}",
                            path.id_paths, route.pool_address, e
                        );
                        let empty_result: Vec<SwapRouteSimulation> = Vec::new();
                        return (empty_result, 0.0);
                    }
                }
            }
        }
    }

    // info!("🔎🔎 Swap path Id: {:?}", path.id_paths);
    info!(
        "🔎🔎💵💵 Precision Simulation: Amount In: {} {} // Amount Out: {} {}",
        amount_begin as f64 / 10_f64.powf(decimals as f64),
        "SOL",
        amount_in as f64 / 10_f64.powf(decimals as f64),
        "SOL"
    );
    let difference = amount_in as f64 - amount_begin as f64;
    info!(
        "🔎🔎 Path simulate {} {} difference",
        difference / 10_f64.powf(decimals as f64),
        "SOL"
    );

    return (swap_simulation_result, difference);
}
