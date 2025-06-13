use crate::common::constants::Env;
use crate::common::utils::{from_pubkey, from_str};
use crate::markets::types::{Dex, DexLabel, Market, PoolItem};
use crate::markets::utils::to_pair_string;
use anyhow::Result;
use log::info;
use reqwest::get;
use serde::{Deserialize, Serialize};
use solana_account_decoder::{UiAccountData, UiAccountEncoding};
use solana_client::nonblocking::rpc_client::RpcClient; // Changed
use solana_client::rpc_config::RpcAccountInfoConfig;
use solana_program::pubkey::Pubkey;
use solana_pubsub_client::pubsub_client::PubsubClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::program_error::ProgramError;
use std::collections::HashMap;
// use std::io::Write; // No longer needed directly for tokio::fs::File
// use std::{fs, fs::File}; // Replaced with tokio::fs
use tokio::fs; // Added
use tokio::io::AsyncWriteExt; // Added for file.write_all(...).await

#[derive(Debug)]
pub struct OrcaDex {
    pub dex: Dex,
    pub pools: Vec<PoolItem>,
}
impl OrcaDex {
    pub async fn new(mut dex: Dex) -> Self { // Changed to async
        let env = Env::new();
        let rpc_client = RpcClient::new(env.rpc_url);

        let mut pools_vec = Vec::new();

        let data =
            fs::read_to_string("src/markets/cache/orca-markets.json").await.expect("Error reading file"); // Changed to await
        let json_value: HashMap<String, Pool> = serde_json::from_str(&data).unwrap();

        // println!("JSON Pools: {:?}", json_value);

        let mut pubkeys_vec: Vec<Pubkey> = Vec::new();

        for (_key, pool) in json_value.clone() {
            let pubkey = from_str(pool.pool_account.as_str()).unwrap();
            pubkeys_vec.push(pubkey);
        }

        let mut results_pools = Vec::new();

        for i in (0..pubkeys_vec.len()).step_by(100) {
            let max_length = std::cmp::min(i + 100, pubkeys_vec.len());
            let batch = &pubkeys_vec[i..max_length];

            let batch_results = rpc_client.get_multiple_accounts(&batch).await.unwrap(); // Changed to await
            for j in batch_results {
                let account = j.unwrap();
                let data = unpack_from_slice(&account.data.into_boxed_slice());
                results_pools.push(data.unwrap());
            }
        }

        for pool in &results_pools {
            let fee = (pool.trade_fee_numerator as f64 / pool.trade_fee_denominator as f64)
                * 10000 as f64;

            let item: PoolItem = PoolItem {
                mint_a: from_pubkey(pool.mint_a),
                mint_b: from_pubkey(pool.mint_b),
                vault_a: from_pubkey(pool.token_account_a),
                vault_b: from_pubkey(pool.token_account_b),
                trade_fee_rate: fee as u128,
            };

            pools_vec.push(item);

            let market: Market = Market {
                token_mint_a: from_pubkey(pool.mint_a),
                token_vault_a: from_pubkey(pool.token_account_a),
                token_mint_b: from_pubkey(pool.mint_b),
                token_vault_b: from_pubkey(pool.token_account_b),
                fee: fee as u64,
                dex_label: DexLabel::Orca,
                id: from_pubkey(pool.token_pool),
                account_data: None,
                liquidity: None,
            };

            let pair_string = to_pair_string(from_pubkey(pool.mint_a), from_pubkey(pool.mint_b));
            if dex.pair_to_markets.contains_key(&pair_string.clone()) {
                let vec_market = dex.pair_to_markets.get_mut(&pair_string).unwrap();
                vec_market.push(market);
            } else {
                dex.pair_to_markets.insert(pair_string, vec![market]);
            }
        }

        info!("Orca: {} pools founded", &results_pools.len());
        Self {
            dex,
            pools: pools_vec,
        }
    }
}

pub async fn fetch_data_orca() -> Result<(), Box<dyn std::error::Error>> {
    let response = get("https://api.orca.so/allPools").await?;
    // info!("response: {:?}", response);
    // info!("response-status: {:?}", response.status().is_success());
    if response.status().is_success() {
        let json: HashMap<String, Pool> = serde_json::from_str(&response.text().await?)?;
        // info!("json: {:?}", json);
        let mut file = fs::File::create("src/markets/cache/orca-markets.json").await?; // Changed to tokio::fs::File and await
        file.write_all(serde_json::to_string(&json)?.as_bytes()).await?; // Changed to await
        info!("Data written to 'orca-markets.json' successfully.");
    } else {
        info!(
            "Fetch of 'orca-markets.json' not successful: {}",
            response.status()
        );
    }
    Ok(())
}

pub async fn stream_orca(account: Pubkey) -> Result<()> {
    let env = Env::new();
    let url = env.wss_rpc_url.as_str();
    let (_account_subscription_client, account_subscription_receiver) =
        PubsubClient::account_subscribe(
            url,
            &account,
            Some(RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::JsonParsed),
                data_slice: None,
                commitment: Some(CommitmentConfig::confirmed()),
                min_context_slot: None,
            }),
        )?;

    loop {
        // To make recv non-blocking, we'd typically spawn_blocking or use an async-aware channel.
        // For a direct minimal change to unblock a tokio runtime, spawn_blocking is an option.
        // However, this changes the function's execution flow.
        // A simple approach if this stream is critical and needs to integrate into a larger async app
        // is to ensure this whole stream_orca function is run on a separate thread if it's meant to be long-lived.
        // Or, more idiomatically, use an async channel if PubsubClient can feed into one.
        // For now, let's make the .recv() itself non-blocking to the current thread if run within tokio::spawn_blocking
        // This is a conceptual placeholder for a more robust async stream handling.
        // The simplest change to avoid blocking the *current* async task if this loop is part of it:
        // This specific change assumes stream_orca is called within a context like tokio::spawn.
        // If stream_orca itself is intended to be a long-running blocking task, it should be spawned onto a dedicated thread.
        // Given the .await in fetch_data_orca, this module is already async-aware.
        // The most direct way to make recv() non-blocking for a tokio runtime is to use spawn_blocking.
        // This implies the processing logic also runs in that blocking thread.
        // If the goal is to integrate into an async stream, a bridge (e.g. tokio::sync::mpsc) would be better.
        // For this refactor, I'll highlight the blocking nature. A full async pubsub client might be needed.
        // The simplest "unblocking" change for a single iteration if this were in an async loop:
        // match tokio::task::spawn_blocking(move || account_subscription_receiver.recv()).await {
        // However, this is for a single recv. The loop structure makes this tricky without a proper async channel.

        // For now, I will leave this as is, but note that account_subscription_receiver.recv() IS BLOCKING.
        // A proper fix would involve a more significant redesign of this streaming part,
        // possibly using something like `tokio_stream::wrappers::ReceiverStream` if the receiver was a tokio mpsc,
        // or bridging std::sync::mpsc to tokio::sync::mpsc.

        match account_subscription_receiver.recv() { // THIS IS STILL BLOCKING
            Ok(response) => {
                // To process data without blocking other async tasks, consider:
                // tokio::spawn(async move { process_data(data_bytes).await });
                let data = response.value.data;
                let bytes_slice = UiAccountData::decode(&data).unwrap();
                // println!("account subscription data response: {:?}", data);
                let account_data = unpack_from_slice(bytes_slice.as_slice());
                println!("Orca Pool updated: {:?}", account);
                println!("Data: {:?}", account_data.unwrap());
            }
            Err(e) => {
                println!("account subscription error: {:?}", e);
                break;
            }
        }
    }

    Ok(())
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    pool: Pool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pool {
    pub pool_id: String,
    pub pool_account: String,
    #[serde(rename = "tokenAAmount")]
    pub token_aamount: String,
    #[serde(rename = "tokenBAmount")]
    pub token_bamount: String,
    pub pool_token_supply: String,
    pub apy: Apy,
    pub volume: Volume,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Apy {
    pub day: String,
    pub week: String,
    pub month: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub day: String,
    pub week: String,
    pub month: String,
}

#[derive(Debug)]
pub struct TokenSwapLayout {
    pub version: u8,
    pub is_initialized: bool,
    pub bump_seed: u8,
    pub pool_token_program_id: Pubkey,
    pub token_account_a: Pubkey,
    pub token_account_b: Pubkey,
    pub token_pool: Pubkey,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub fee_account: Pubkey,
    pub trade_fee_numerator: u64,
    pub trade_fee_denominator: u64,
    pub owner_trade_fee_numerator: u64,
    pub owner_trade_fee_denominator: u64,
    pub owner_withdraw_fee_numerator: u64,
    pub owner_withdraw_fee_denominator: u64,
    pub host_fee_numerator: u64,
    pub host_fee_denominator: u64,
    pub curve_type: u8,
    pub curve_parameters: [u8; 32],
}

fn unpack_from_slice(src: &[u8]) -> Result<TokenSwapLayout, ProgramError> {
    let version = src[0];
    let is_initialized = src[1] != 0;
    let bump_seed = src[2];
    let pool_token_program_id =
        Pubkey::new_from_array(<[u8; 32]>::try_from(&src[3..35]).expect("Orca pools bad unpack"));
    let token_account_a =
        Pubkey::new_from_array(<[u8; 32]>::try_from(&src[35..67]).expect("Orca pools bad unpack"));
    let token_account_b =
        Pubkey::new_from_array(<[u8; 32]>::try_from(&src[67..99]).expect("Orca pools bad unpack"));
    let token_pool =
        Pubkey::new_from_array(<[u8; 32]>::try_from(&src[99..131]).expect("Orca pools bad unpack"));
    let mint_a = Pubkey::new_from_array(
        <[u8; 32]>::try_from(&src[131..163]).expect("Orca pools bad unpack"),
    );
    let mint_b = Pubkey::new_from_array(
        <[u8; 32]>::try_from(&src[163..195]).expect("Orca pools bad unpack"),
    );
    let fee_account = Pubkey::new_from_array(
        <[u8; 32]>::try_from(&src[195..227]).expect("Orca pools bad unpack"),
    );
    let trade_fee_numerator =
        u64::from_le_bytes(<[u8; 8]>::try_from(&src[227..235]).expect("Orca pools bad unpack"));
    let trade_fee_denominator =
        u64::from_le_bytes(<[u8; 8]>::try_from(&src[235..243]).expect("Orca pools bad unpack"));
    let owner_trade_fee_numerator =
        u64::from_le_bytes(<[u8; 8]>::try_from(&src[243..251]).expect("Orca pools bad unpack"));
    let owner_trade_fee_denominator =
        u64::from_le_bytes(<[u8; 8]>::try_from(&src[251..259]).expect("Orca pools bad unpack"));
    let owner_withdraw_fee_numerator =
        u64::from_le_bytes(<[u8; 8]>::try_from(&src[259..267]).expect("Orca pools bad unpack"));
    let owner_withdraw_fee_denominator =
        u64::from_le_bytes(<[u8; 8]>::try_from(&src[267..275]).expect("Orca pools bad unpack"));
    let host_fee_numerator =
        u64::from_le_bytes(<[u8; 8]>::try_from(&src[275..283]).expect("Orca pools bad unpack"));
    let host_fee_denominator =
        u64::from_le_bytes(<[u8; 8]>::try_from(&src[283..291]).expect("Orca pools bad unpack"));
    let curve_type = src[291];
    let mut curve_parameters = [0u8; 32];
    curve_parameters.copy_from_slice(&src[292..]);

    Ok(TokenSwapLayout {
        version,
        is_initialized,
        bump_seed,
        pool_token_program_id,
        token_account_a,
        token_account_b,
        token_pool,
        mint_a,
        mint_b,
        fee_account,
        trade_fee_numerator,
        trade_fee_denominator,
        owner_trade_fee_numerator,
        owner_trade_fee_denominator,
        owner_withdraw_fee_numerator,
        owner_withdraw_fee_denominator,
        host_fee_numerator,
        host_fee_denominator,
        curve_type,
        curve_parameters,
    })
}
