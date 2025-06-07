use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::Arc;

use anyhow::Result;
use futures::FutureExt;
use log::{error, info};
use rust_socketio::{
    asynchronous::{Client, ClientBuilder},
    Payload,
};
use solana_sdk::commitment_config::CommitmentConfig;
use tokio::task::JoinSet;
use MEV_Bot_Solana::arbitrage::strategies::{
    optimism_tx_strategy, run_arbitrage_strategy, sorted_interesting_path_strategy,
};
use MEV_Bot_Solana::arbitrage::types::{
    SwapPathSelected, TokenInArb, TokenInfos, VecSwapPathSelected,
};
use MEV_Bot_Solana::common::config::{
    Config, STRATEGY_BEST_PATH, STRATEGY_MASSIVE, STRATEGY_OPTIMISM,
};
use MEV_Bot_Solana::common::constants::Env;
use MEV_Bot_Solana::common::database::insert_vec_swap_path_selected_collection;
use MEV_Bot_Solana::common::types::InputVec;
use MEV_Bot_Solana::common::utils::{get_tokens_infos, setup_logger};
use MEV_Bot_Solana::execution::executor::{ExecutionQueue, TransactionExecutor};
use MEV_Bot_Solana::fees::priority_fees::{init_global_fee_service, PriorityFeeConfig};
use MEV_Bot_Solana::markets::pools::load_all_pools;

#[tokio::main]
async fn main() -> Result<()> {
    // Load the configuration from config.json
    let config = Config::load().expect("Failed to load configuration");

    dotenv::dotenv().ok();
    setup_logger().unwrap();

    info!("Starting MEV_Bot_Solana");

    // Initialize the global priority fee service
    let env = Env::new();
    let rpc_client_fees = Arc::new(
        solana_client::nonblocking::rpc_client::RpcClient::new_with_commitment(
            config
                .rpc_url_tx
                .clone()
                .unwrap_or_else(|| env.rpc_url_tx.clone()),
            CommitmentConfig::processed(),
        ),
    );

    let fee_config = PriorityFeeConfig {
        mode: config.fee_mode,
        cache_duration_secs: config.fee_cache_duration_secs,
        custom_strategy: None,
    };

    init_global_fee_service(rpc_client_fees, fee_config).expect("Failed to initialize fee service");
    info!("✅ Priority fee service initialized");

    // Initialize the transaction executor
    let executor = TransactionExecutor::new(config.execution_mode)?;
    let execution_queue = ExecutionQueue::new(executor.get_sender());

    // Start the executor in background
    let executor_handle = tokio::spawn(async move {
        if let Err(e) = executor.run().await {
            error!("Executor failed: {:?}", e);
        }
    });
    info!("⚠️⚠️ New fresh pools fetched on METEORA and RAYDIUM are excluded because a lot of time there have very low liquidity, potentially can be used on subscribe log strategy");
    info!("⚠️⚠️ Liquidity is fetch to API and can be outdated on Radyium Pool");

    // Convert input_vectors to the legacy InputVec format
    let inputs_vec: Vec<InputVec> = config
        .input_vectors
        .iter()
        .map(|input| {
            // Convert TokenConfig to TokenInArb
            let tokens_to_arb: Vec<TokenInArb> = input
                .tokens_to_arb
                .iter()
                .map(|token_config| TokenInArb {
                    address: token_config.address.clone(),
                    symbol: token_config.symbol.clone(),
                })
                .collect();
            
            InputVec {
                tokens_to_arb,
                include_1hop: input.include_1hop,
                include_2hop: input.include_2hop,
                numbers_of_best_paths: input.numbers_of_best_paths,
                get_fresh_pools_bool: input.get_fresh_pools_bool,
            }
        })
        .collect();

    // The first token is the base token (here SOL)
    let tokens_to_arb: Vec<TokenInArb> = inputs_vec
        .clone()
        .into_iter()
        .flat_map(|input| input.tokens_to_arb)
        .collect();

    // WebSocket connection - only connect if WSS_RPC_URL is configured
    let wss_rpc_url = config
        .wss_rpc_url
        .clone()
        .unwrap_or_else(|| env.wss_rpc_url.clone());
    if !wss_rpc_url.is_empty() {
        info!("Open Socket IO channel to: {}", wss_rpc_url);

        let callback = |payload: Payload, _socket: Client| {
            async move {
                match payload {
                    Payload::Text(data) => println!("Received: {:?}", data),
                    Payload::Binary(bin_data) => println!("Received bytes: {:#?}", bin_data),
                    // Use Text instead of deprecated String
                    _ => println!("Received other payload type"),
                }
            }
            .boxed()
        };

        match ClientBuilder::new(&wss_rpc_url)
            .namespace("/")
            .on("connection", callback)
            .on("error", |err, _| {
                async move { eprintln!("Error: {:#?}", err) }.boxed()
            })
            .on("orca_quote", callback)
            .on("orca_quote_res", callback)
            .connect()
            .await
        {
            Ok(_socket) => {
                info!("✅ WebSocket connected successfully");
                // Store socket if needed for later use
            }
            Err(e) => {
                error!(
                    "⚠️ WebSocket connection failed: {:?}. Continuing without WebSocket.",
                    e
                );
            }
        }
    } else {
        info!("WebSocket URL not configured, skipping WebSocket connection");
    }

    // Initialize a JoinSet for parallel strategy execution
    let mut set: JoinSet<()> = JoinSet::new();

    // Launch strategies in parallel
    if config.contains_strategy(STRATEGY_MASSIVE) {
        info!("🏊 Launch pools fetching infos...");
        let dexs = load_all_pools(config.fetch_new_pools).await;
        info!("🏊 {} Dexs are loaded", dexs.len());
        info!("🪙🪙 Tokens Infos: {:?}", tokens_to_arb);

        // Clone required data for the task
        let inputs_vec_clone = inputs_vec.clone();
        let execution_queue_clone = execution_queue.clone();
        let config_clone = config.clone();
        let dexs_clone = dexs.clone();

        // Spawn massive strategy as a background task
        set.spawn(async move {
            info!("📈 Launch massive arbitrage process...");
            let mut vec_best_paths: Vec<String> = Vec::new();

            for input_iter in inputs_vec_clone.clone() {
                let tokens_infos: HashMap<String, TokenInfos> =
                    get_tokens_infos(input_iter.tokens_to_arb.clone()).await;

                let result = run_arbitrage_strategy(
                    config_clone.simulation_amount,
                    input_iter.get_fresh_pools_bool,
                    config_clone.restrict_sol_usdc,
                    input_iter.include_1hop,
                    input_iter.include_2hop,
                    input_iter.numbers_of_best_paths,
                    dexs_clone.clone(),
                    input_iter.tokens_to_arb.clone(),
                    tokens_infos.clone(),
                    Some(&execution_queue_clone),
                )
                .await;

                if let Ok((path_for_best_strategie, _)) = result {
                    vec_best_paths.push(path_for_best_strategie);
                }
            }

            if inputs_vec_clone.clone().len() > 1 {
                let mut vec_to_ultra_strat: Vec<SwapPathSelected> = Vec::new();
                let mut ultra_strat_name: String = String::new();

                for (index, iter_path) in vec_best_paths.iter().enumerate() {
                    let name_raw: Vec<&str> = iter_path.split('/').collect();
                    let name: Vec<&str> = name_raw[1].split('.').collect();
                    if index == 0 {
                        ultra_strat_name = format!("{}-{}", index, name[0]);
                    } else {
                        ultra_strat_name = format!("{}-{}-{}", ultra_strat_name, index, name[0]);
                    }

                    if let Ok(file_read) = OpenOptions::new().read(true).write(true).open(iter_path)
                    {
                        if let Ok(paths_vec) =
                            serde_json::from_reader::<_, VecSwapPathSelected>(&file_read)
                        {
                            for sp_iter in paths_vec.value {
                                vec_to_ultra_strat.push(sp_iter);
                            }
                        }
                    }
                }

                let path = format!(
                    "{}/ultra_strategies/{}.json",
                    config_clone.output_dir, ultra_strat_name
                );
                if let Ok(_) = File::create(path.clone()) {
                    if let Ok(file) = OpenOptions::new().read(true).write(true).open(path.clone()) {
                        let mut writer = BufWriter::new(&file);
                        let content = VecSwapPathSelected {
                            value: vec_to_ultra_strat.clone(),
                        };

                        if let Ok(json_string) = serde_json::to_string(&content) {
                            if writer.write_all(json_string.as_bytes()).is_ok()
                                && writer.flush().is_ok()
                            {
                                info!("Data written to '{}' successfully.", path);

                                let _ = insert_vec_swap_path_selected_collection(
                                    "ultra_strategies",
                                    content,
                                )
                                .await;

                                // Update the path_best_strategie in the config
                                let mut updated_config = config_clone.clone();
                                updated_config.path_best_strategie = path;
                                if let Err(e) = updated_config.save() {
                                    error!("Failed to save updated config: {:?}", e);
                                }
                            }
                        }
                    }
                }
            }

            info!("✅ Massive strategy completed");
        });
    }

    // Launch BestPath strategy in parallel if enabled
    let should_run_bestpath_alone =
        !config.contains_strategy(STRATEGY_MASSIVE) && config.contains_strategy(STRATEGY_BEST_PATH);

    if should_run_bestpath_alone {
        // Clone required data
        let tokens_to_arb_clone = tokens_to_arb.clone();
        let execution_queue_clone = execution_queue.clone();
        let config_clone = config.clone();

        set.spawn(async move {
            info!("📈 Launch best path strategy...");
            let tokens_infos: HashMap<String, TokenInfos> =
                get_tokens_infos(tokens_to_arb_clone.clone()).await;

            let _ = sorted_interesting_path_strategy(
                config_clone.simulation_amount,
                config_clone.path_best_strategie.clone(),
                tokens_to_arb_clone.clone(),
                tokens_infos.clone(),
                Some(&execution_queue_clone),
            )
            .await;

            info!("✅ Best path strategy completed");
        });
    }

    // Launch Optimism strategy in parallel if enabled
    if config.contains_strategy(STRATEGY_OPTIMISM) {
        let execution_queue_clone = execution_queue.clone();
        let optimism_path = config.optimism_path.clone();

        set.spawn(async move {
            info!("📈 Launch optimism strategy...");
            let _ = optimism_tx_strategy(optimism_path, Some(&execution_queue_clone)).await;
            info!("✅ Optimism strategy completed");
        });
    }

    // Wait for all tasks to complete
    while let Some(res) = set.join_next().await {
        if let Err(e) = res {
            error!("Strategy task error: {:?}", e);
        }
    }

    // Cancel executor on shutdown
    executor_handle.abort();

    println!("End");
    Ok(())
}
