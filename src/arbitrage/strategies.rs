use super::{
    simulate::simulate_path_precision,
    types::{SwapPath, TokenInArb, TokenInfos},
};
use crate::markets::types::{Dex, Market};
use crate::{
    arbitrage::{
        calc_arb::{calculate_arb, get_markets_arb},
        simulate::simulate_path,
        streams::get_fresh_accounts_states,
        types::{
            SwapPathResult, SwapPathSelected, SwapRouteSimulation, VecSwapPathResult,
            VecSwapPathSelected,
        },
    },
    common::{
        database::{insert_swap_path_result_collection, insert_vec_swap_path_selected_collection},
        utils::write_file_swap_path_result,
    },
    transactions::create_transaction::{
        create_and_send_swap_transaction, ChainType, SendOrSimulate,
    },
};
use anyhow::Result;
use chrono::{Datelike, Utc};
use futures::future::join_all;
use indicatif::{ProgressBar, ProgressStyle};
use log::{error, info};
use rust_socketio::asynchronous::Client;
use std::io::{BufWriter, Write};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    sync::Arc,
};
use tokio::sync::Semaphore;

// NEW: Use our execution system instead of TCP
use crate::execution::executor::{execute_profitable_swap, ExecutionMode, ExecutionQueue};

pub async fn run_arbitrage_strategy(
    simulation_amount: u64,
    get_fresh_pools_bool: bool,
    restrict_sol_usdc: bool,
    include_1hop: bool,
    include_2hop: bool,
    numbers_of_best_paths: usize,
    dexs: Vec<Dex>,
    tokens: Vec<TokenInArb>,
    tokens_infos: HashMap<String, TokenInfos>,
    execution_queue: Option<&ExecutionQueue>, // NEW: Optional execution queue
) -> Result<(String, VecSwapPathSelected)> {
    info!("👀 Run Arbitrage Strategies...");

    let markets_arb = get_markets_arb(
        get_fresh_pools_bool,
        restrict_sol_usdc,
        dexs,
        tokens.clone(),
    )
    .await;

    // Sort markets with low liquidity
    let (sorted_markets_arb, all_paths) = calculate_arb(
        include_1hop,
        include_2hop,
        markets_arb.clone(),
        tokens.clone(),
    );

    //Get fresh account state
    let fresh_markets_arb = get_fresh_accounts_states(sorted_markets_arb.clone()).await;

    // We keep route simulation result for RPC optimization
    let route_simulation: HashMap<Vec<u32>, Vec<SwapRouteSimulation>> = HashMap::new();
    let mut swap_paths_results: VecSwapPathResult = VecSwapPathResult { result: Vec::new() };

    let mut counter_failed_paths = 0;
    let mut counter_positive_paths = 0;
    let mut error_paths: HashMap<Vec<u32>, u8> = HashMap::new();

    //Progress bar
    let bar = ProgressBar::new(all_paths.len() as u64);
    bar.set_style(
        ProgressStyle::with_template("[{elapsed}] [{bar:140.cyan/blue}] ✅ {pos:>3}/{len:3} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );
    bar.set_message(format!(
        "❌ Failed routes: {}/{} 💸 Positive routes: {}/{}",
        counter_failed_paths,
        bar.position(),
        counter_positive_paths,
        bar.position()
    ));

    let mut best_paths_for_strat: Vec<SwapPathSelected> = Vec::new();
    let mut return_path = "".to_string();
    let mut counter_sp_result = 0;

    // Create a semaphore to limit concurrent simulations (avoid RPC rate limits)
    let semaphore = Arc::new(Semaphore::new(20)); // Adjust based on RPC capacity

    // Wrap large read-only data in Arc to avoid repeated cloning
    let tokens_infos_arc = Arc::new(tokens_infos);
    let fresh_markets_arb_arc = Arc::new(fresh_markets_arb);
    let route_simulation_arc = Arc::new(tokio::sync::Mutex::new(route_simulation));

    // Process paths in batches to avoid memory issues
    for chunk in all_paths.chunks(50) {
        let mut simulation_tasks = Vec::new();

        // Prepare the tasks
        for path in chunk {
            // Verify error in previous paths to see if this path is interesting
            let key = vec![path.id_paths[0], path.id_paths[1]];
            let counter_opt = error_paths.get(&key.clone());
            if let Some(value) = counter_opt {
                if value >= &3 {
                    error!(
                        "🔴⏭️  Skip the {:?} path because previous errors",
                        path.id_paths
                    );
                    counter_failed_paths += 1;
                    continue;
                }
            }

            let path = path.clone();
            let semaphore = Arc::clone(&semaphore);
            let tokens_infos = Arc::clone(&tokens_infos_arc);
            let fresh_markets_arb = Arc::clone(&fresh_markets_arb_arc);
            let route_simulation = Arc::clone(&route_simulation_arc);
            let simulation_amount = simulation_amount;

            let task = tokio::spawn(async move {
                // Acquire a permit from the semaphore
                let _permit = semaphore.acquire().await.unwrap();

                // Get Pubkeys of the concerned markets
                let pubkeys: Vec<String> = path
                    .paths
                    .clone()
                    .iter()
                    .map(|route| route.clone().pool_address)
                    .collect();
                let markets: Vec<Market> = pubkeys
                    .iter()
                    .filter_map(|key| fresh_markets_arb.get(key))
                    .cloned()
                    .collect();

                // Get the current route simulation
                let route_simulation_current = route_simulation.lock().await.clone();

                // Simulate the path
                let (new_route_simulation, swap_simulation_result, result_difference) =
                    simulate_path(
                        simulation_amount,
                        path.clone(),
                        markets.clone(),
                        tokens_infos.as_ref().clone(),
                        route_simulation_current,
                    )
                    .await;

                // Return the simulation results and path info
                (
                    path,
                    markets,
                    new_route_simulation,
                    swap_simulation_result,
                    result_difference,
                )
            });

            simulation_tasks.push(task);
        }

        // Wait for all simulations in this batch to complete
        let results = join_all(simulation_tasks).await;

        // Process results and update route_simulation
        for result in results {
            if let Ok((
                path,
                markets,
                new_route_simulation,
                swap_simulation_result,
                result_difference,
            )) = result
            {
                // Update the route simulation with the new entries
                let mut route_sim = route_simulation_arc.lock().await;
                route_sim.extend(new_route_simulation);
                drop(route_sim); // Release the lock

                // Update progress bar
                bar.inc(1);

                // If no error in swap path
                if swap_simulation_result.len() >= path.hops as usize {
                    let mut tokens_path = swap_simulation_result
                        .iter()
                        .map(|swap_sim| {
                            tokens_infos_arc
                                .get(&swap_sim.token_in)
                                .unwrap()
                                .symbol
                                .clone()
                        })
                        .collect::<Vec<String>>()
                        .join("-");
                    tokens_path = format!("{}-{}", tokens_path, tokens[0].symbol.clone());

                    let sp_result: SwapPathResult = SwapPathResult {
                        path_id: bar.position() as u32,
                        hops: path.hops,
                        tokens_path: tokens_path.clone(),
                        route_simulations: swap_simulation_result.clone(),
                        token_in: tokens[0].address.clone(),
                        token_in_symbol: tokens[0].symbol.clone(),
                        token_out: tokens[0].address.clone(),
                        token_out_symbol: tokens[0].symbol.clone(),
                        amount_in: swap_simulation_result[0].amount_in.clone(),
                        estimated_amount_out: swap_simulation_result
                            [swap_simulation_result.len() - 1]
                            .estimated_amount_out
                            .clone(),
                        estimated_min_amount_out: swap_simulation_result
                            [swap_simulation_result.len() - 1]
                            .estimated_min_amount_out
                            .clone(),
                        result: result_difference,
                    };
                    swap_paths_results.result.push(sp_result.clone());

                    // NEW: Use execution queue instead of TCP
                    if let Some(queue) = execution_queue {
                        if result_difference > 20000000.0 {
                            println!(
                                "💸💸💸💸💸💸💸💸💸 Profitable swap detected 💸💸💸💸💸💸💸💸💸"
                            );
                            info!(
                                "💸 Sending to execution queue: {} SOL profit",
                                result_difference / 1e9
                            );

                            let now = Utc::now();
                            let date = format!("{}-{}-{}", now.day(), now.month(), now.year());
                            let path = format!(
                                "optimism_transactions/{}-{}-{}.json",
                                date,
                                tokens_path.clone(),
                                counter_sp_result
                            );

                            let _ = insert_swap_path_result_collection(
                                "optimism_transactions",
                                sp_result.clone(),
                            )
                            .await;
                            let _ = write_file_swap_path_result(path.clone(), sp_result.clone());
                            counter_sp_result += 1;

                            // Execute via our new system
                            let mode = ExecutionMode::Live; // Or Paper/Simulate based on config
                            let _ = execute_profitable_swap(sp_result, queue, mode).await;
                        }
                    }

                    // Reset errors if one path is good
                    let key = vec![path.id_paths[0], path.id_paths[1]];
                    error_paths.insert(key, 0);

                    // Custom Queue FIFO for best results
                    if best_paths_for_strat.len() < numbers_of_best_paths {
                        best_paths_for_strat.push(SwapPathSelected {
                            result: result_difference,
                            path: path.clone(),
                            markets: markets,
                        });
                        if best_paths_for_strat.len() == numbers_of_best_paths {
                            best_paths_for_strat
                                .sort_by(|a, b| b.result.partial_cmp(&a.result).unwrap());
                        }
                    } else if result_difference
                        > best_paths_for_strat[best_paths_for_strat.len() - 1].result
                    {
                        for (index, path_in_vec) in best_paths_for_strat.clone().iter().enumerate()
                        {
                            if result_difference < path_in_vec.result {
                                continue;
                            } else {
                                best_paths_for_strat[index] = SwapPathSelected {
                                    result: result_difference,
                                    path: path.clone(),
                                    markets: markets,
                                };
                                break;
                            }
                        }
                    }

                    // Update positive routes
                    if result_difference > 0.0 {
                        counter_positive_paths += 1;
                        bar.set_message(format!(
                            "❌ Failed routes: {}/{} 💸 Positive routes: {}/{}",
                            counter_failed_paths,
                            bar.position(),
                            counter_positive_paths,
                            bar.position()
                        ));
                    }
                } else {
                    // If simulation failed, mark as failed
                    counter_failed_paths += 1;

                    // Update error paths tracking
                    if swap_simulation_result.len() == 0 {
                        let key = vec![path.id_paths[0], path.id_paths[1]];
                        let counter_opt = error_paths.get(&key.clone());
                        match counter_opt {
                            None => {
                                error_paths.insert(key, 1);
                            }
                            Some(value) => {
                                error_paths.insert(key, value + 1);
                            }
                        }
                    }

                    // Update progress bar
                    bar.set_message(format!(
                        "❌ Failed routes: {}/{} 💸 Positive routes: {}/{}",
                        counter_failed_paths,
                        bar.position(),
                        counter_positive_paths,
                        bar.position()
                    ));
                }

                // Print best paths periodically
                let position = bar.position() as usize;
                if position % 10 == 0 && position > 0 {
                    println!(
                        "best_paths_for_strat {:#?}",
                        best_paths_for_strat
                            .iter()
                            .map(|iter| iter.result)
                            .collect::<Vec<f64>>()
                    );
                }
            }
        }

        // Write intermediate results to file
        let position = bar.position() as usize;
        if (position != 0 && position % 300 == 0) || position == all_paths.len() {
            let file_number = position / 300;
            let symbols = tokens
                .iter()
                .map(|token| &token.symbol)
                .cloned()
                .collect::<Vec<String>>()
                .join("-");
            let mut file =
                File::create(format!("results/result_{}_{}.json", file_number, symbols)).unwrap();
            match serde_json::to_writer_pretty(&mut file, &swap_paths_results) {
                Ok(_) => {
                    info!("🥇🥇 Results written!");
                    swap_paths_results = VecSwapPathResult { result: Vec::new() };
                }
                Err(value) => {
                    error!("Results not written properly: {:?}", value);
                }
            };
        }
    }

    let mut tokens_list = "".to_string();
    for (index, token) in tokens.iter().enumerate() {
        if index == 0 {
            tokens_list = format!("{}", tokens[index].symbol.clone());
        } else {
            tokens_list = format!("{}-{}", tokens_list, tokens[index].symbol.clone());
        }
    }

    let path = format!("best_paths_selected/{}.json", tokens_list);
    let _ = File::create(path.clone());

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.clone())?;
    let mut writer = BufWriter::new(&file);

    let content = VecSwapPathSelected {
        value: best_paths_for_strat.clone(),
    };
    writer.write_all(serde_json::to_string(&content)?.as_bytes())?;
    writer.flush()?;
    info!("Data written to '{}' successfully.", path);

    let _ = insert_vec_swap_path_selected_collection("best_paths_selected", content.clone()).await;

    return_path = path;
    bar.finish();
    Ok((
        return_path,
        VecSwapPathSelected {
            value: best_paths_for_strat,
        },
    ))
}

pub async fn precision_strategy(
    socket: Client,
    path: SwapPath,
    markets: Vec<Market>,
    tokens: Vec<TokenInArb>,
    tokens_infos: HashMap<String, TokenInfos>,
) {
    info!(
        "🔎🔎 Run a Precision Simulation on Path Id: {:?}",
        path.id_paths
    );

    let mut swap_paths_results: VecSwapPathResult = VecSwapPathResult { result: Vec::new() };

    let decimals = 9;
    let amounts_simulations = vec![
        5 * 10_u64.pow(decimals - 1),
        1 * 10_u64.pow(decimals),
        5 * 10_u64.pow(decimals),
        10 * 10_u64.pow(decimals),
        20 * 10_u64.pow(decimals),
    ];

    let mut result_amt = 0.0;
    let mut _sp_to_tx: Option<SwapPathResult> = None;

    for (index, amount_in) in amounts_simulations.iter().enumerate() {
        let (swap_simulation_result, result_difference) = simulate_path_precision(
            amount_in.clone(),
            socket.clone(),
            path.clone(),
            markets.clone(),
            tokens_infos.clone(),
        )
        .await;

        if swap_simulation_result.len() >= path.hops as usize {
            let mut tokens_path = swap_simulation_result
                .iter()
                .map(|swap_sim| tokens_infos.get(&swap_sim.token_in).unwrap().symbol.clone())
                .collect::<Vec<String>>()
                .join("-");
            tokens_path = format!("{}-{}", tokens_path, tokens[0].symbol.clone());

            let sp_result: SwapPathResult = SwapPathResult {
                path_id: index as u32,
                hops: path.hops,
                tokens_path: tokens_path,
                route_simulations: swap_simulation_result.clone(),
                token_in: tokens[0].address.clone(),
                token_in_symbol: tokens[0].symbol.clone(),
                token_out: tokens[0].address.clone(),
                token_out_symbol: tokens[0].symbol.clone(),
                amount_in: swap_simulation_result[0].amount_in.clone(),
                estimated_amount_out: swap_simulation_result[swap_simulation_result.len() - 1]
                    .estimated_amount_out
                    .clone(),
                estimated_min_amount_out: swap_simulation_result[swap_simulation_result.len() - 1]
                    .estimated_min_amount_out
                    .clone(),
                result: result_difference,
            };
            swap_paths_results.result.push(sp_result.clone());

            if result_difference > result_amt {
                result_amt = result_difference;
                println!("result_amt: {}", result_amt);
                _sp_to_tx = Some(sp_result.clone());
            }
        }
    }
}

pub async fn sorted_interesting_path_strategy(
    simulation_amount: u64,
    path: String,
    tokens: Vec<TokenInArb>,
    tokens_infos: HashMap<String, TokenInfos>,
    execution_queue: Option<&ExecutionQueue>, // NEW: Optional execution queue
) -> Result<()> {
    let file_read = OpenOptions::new().read(true).write(true).open(path)?;
    let paths_vec: VecSwapPathSelected = serde_json::from_reader(&file_read).unwrap();
    let mut counter_sp_result = 0;

    let paths: Vec<SwapPathSelected> = paths_vec.value;
    let route_simulation: HashMap<Vec<u32>, Vec<SwapRouteSimulation>> = HashMap::new();

    loop {
        for (index, path) in paths.iter().enumerate() {
            let (new_route_simulation, swap_simulation_result, result_difference) = simulate_path(
                simulation_amount,
                path.path.clone(),
                path.markets.clone(),
                tokens_infos.clone(),
                route_simulation.clone(),
            )
            .await;

            //If no error in swap path
            if swap_simulation_result.len() >= path.path.hops as usize {
                let mut tokens_path = swap_simulation_result
                    .iter()
                    .map(|swap_sim| tokens_infos.get(&swap_sim.token_in).unwrap().symbol.clone())
                    .collect::<Vec<String>>()
                    .join("-");
                tokens_path = format!("{}-{}", tokens_path, tokens[0].symbol.clone());

                let sp_result: SwapPathResult = SwapPathResult {
                    path_id: index as u32,
                    hops: path.path.hops,
                    tokens_path: tokens_path.clone(),
                    route_simulations: swap_simulation_result.clone(),
                    token_in: tokens[0].address.clone(),
                    token_in_symbol: tokens[0].symbol.clone(),
                    token_out: tokens[0].address.clone(),
                    token_out_symbol: tokens[0].symbol.clone(),
                    amount_in: swap_simulation_result[0].amount_in.clone(),
                    estimated_amount_out: swap_simulation_result[swap_simulation_result.len() - 1]
                        .estimated_amount_out
                        .clone(),
                    estimated_min_amount_out: swap_simulation_result
                        [swap_simulation_result.len() - 1]
                        .estimated_min_amount_out
                        .clone(),
                    result: result_difference,
                };

                // NEW: Use execution queue instead of TCP
                if let Some(queue) = execution_queue {
                    if result_difference > 20000000.0 {
                        println!("💸💸💸💸💸💸💸💸💸 Profitable swap detected 💸💸💸💸💸💸💸💸💸");
                        info!(
                            "💸 Sending to execution queue: {} SOL profit",
                            result_difference / 1e9
                        );

                        let now = Utc::now();
                        let date = format!("{}-{}-{}", now.day(), now.month(), now.year());
                        let path = format!(
                            "optimism_transactions/{}-{}-{}.json",
                            date, tokens_path, counter_sp_result
                        );

                        let _ = write_file_swap_path_result(path.clone(), sp_result.clone());
                        counter_sp_result += 1;

                        // Execute via our new system
                        let mode = ExecutionMode::Live; // Or Paper/Simulate based on config
                        let _ = execute_profitable_swap(sp_result, queue, mode).await;
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }
    }
}

pub async fn optimism_tx_strategy(
    path: String,
    execution_queue: Option<&ExecutionQueue>,
) -> Result<()> {
    let file_read = OpenOptions::new().read(true).write(true).open(path)?;
    let spr: SwapPathResult = serde_json::from_reader(&file_read).unwrap();

    println!("💸💸💸💸💸💸💸💸💸 Begin Execute the tx 💸💸💸💸💸💸💸💸💸");

    // NEW: Use execution queue if available
    if let Some(queue) = execution_queue {
        let mode = ExecutionMode::Live;
        execute_profitable_swap(spr, queue, mode).await?;
    } else {
        // Fallback to direct execution
        let _ =
            create_and_send_swap_transaction(SendOrSimulate::Send, ChainType::Mainnet, spr.clone())
                .await;
    }

    Ok(())
}
