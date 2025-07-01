use crate::common::constants::Env;
use crate::common::utils::{from_pubkey, from_str};
use crate::markets::types::{Dex, DexLabel, Market, PoolItem};
use crate::markets::utils::to_pair_string;
use anyhow::{Result, Context};
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
    pub async fn new(mut dex: Dex) -> Result<Self> {
        let env = Env::new();
        let rpc_client = RpcClient::new(env.rpc_url);

        let mut pools_vec = Vec::new();

        let data = fs::read_to_string("src/markets/cache/orca-markets.json")
            .await
            .context("Failed to read orca-markets.json cache file")?;
        let json_value: HashMap<String, Pool> =
            serde_json::from_str(&data).context("Failed to parse orca-markets.json")?;

        let mut pubkeys_vec: Vec<Pubkey> = Vec::new();

        // Iterate over values directly to avoid cloning the whole HashMap
        for pool in json_value.values() {
            let pubkey = from_str(pool.pool_account.as_str())
                .map_err(|e| anyhow::anyhow!("Failed to parse pool_account string to Pubkey: {}", e))?;
            pubkeys_vec.push(pubkey);
        }

        let mut results_pools = Vec::new();

        for chunk in pubkeys_vec.chunks(100) {
            let batch_results = rpc_client
                .get_multiple_accounts(chunk)
                .await
                .context("Failed to get multiple accounts from RPC")?;

            for account_option in batch_results {
                let account = account_option.context("Failed to get account, RPC returned None")?; // Or handle as skippable
                let token_swap_data = unpack_from_slice(&account.data.into_boxed_slice())?;
                results_pools.push(token_swap_data);
            }
        }

        for pool_data in &results_pools {
            let fee_rate_float = if pool_data.trade_fee_denominator != 0 {
                (pool_data.trade_fee_numerator as f64 / pool_data.trade_fee_denominator as f64) * 10000.0 // Assuming fee is in basis points
            } else {
                0.0 // Avoid division by zero
            };

            let item: PoolItem = PoolItem {
                mint_a: from_pubkey(pool_data.mint_a),
                mint_b: from_pubkey(pool_data.mint_b),
                vault_a: from_pubkey(pool_data.token_account_a),
                vault_b: from_pubkey(pool_data.token_account_b),
                trade_fee_rate: fee_rate_float as u128, // Consider if u128 is appropriate for basis points
            };
            pools_vec.push(item);

            let market: Market = Market {
                token_mint_a: from_pubkey(pool_data.mint_a),
                token_vault_a: from_pubkey(pool_data.token_account_a),
                token_mint_b: from_pubkey(pool_data.mint_b),
                token_vault_b: from_pubkey(pool_data.token_account_b),
                fee: fee_rate_float as u64, // Store fee in consistent unit (e.g. basis points)
                dex_label: DexLabel::Orca,
                id: from_pubkey(pool_data.token_pool),
                account_data: None, // Explicitly None, as per original logic for new()
                liquidity: None,    // Explicitly None
            };

            let pair_string = to_pair_string(
                from_pubkey(pool_data.mint_a),
                from_pubkey(pool_data.mint_b),
            );

            // Use entry API for cleaner insertion/update
            dex.pair_to_markets
                .entry(pair_string)
                .or_default()
                .push(market);
        }

        info!("Orca: {} pools founded", results_pools.len());
        Ok(Self {
            dex,
            pools: pools_vec,
        })
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
                let ui_account_data = response.value.data;
                match UiAccountData::decode(&ui_account_data) {
                    Ok(bytes_slice) => {
                        match unpack_from_slice(bytes_slice.as_slice()) {
                            Ok(account_data) => {
                                // Successfully decoded and unpacked
                                println!("Orca Pool updated: {:?}", account);
                                println!("Data: {:?}", account_data);
                            }
                            Err(e) => {
                                // Error unpacking the data
                                eprintln!(
                                    "Error unpacking Orca pool data for account {:?}: {:?}. Raw data: {:?}",
                                    account, e, bytes_slice
                                );
                            }
                        }
                    }
                    Err(e) => {
                        // Error decoding UiAccountData
                        eprintln!(
                            "Error decoding UiAccountData for Orca pool {:?}: {:?}. Original data: {:?}",
                            account, e, ui_account_data
                        );
                    }
                }
            }
            Err(e) => {
                // Error receiving from subscription
                eprintln!("Orca account subscription error for {:?}: {:?}", account, e);
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
    // Helper closure to convert slice to array and then to Pubkey
    let to_pubkey = |slice: &[u8]| -> Result<Pubkey, ProgramError> {
        slice
            .try_into()
            .map(Pubkey::new_from_array)
            .map_err(|_| ProgramError::InvalidAccountData) // Map SliceTryFromError to ProgramError
    };

    // Helper closure to convert slice to array and then to u64
    let to_u64 = |slice: &[u8]| -> Result<u64, ProgramError> {
        slice
            .try_into()
            .map(u64::from_le_bytes)
            .map_err(|_| ProgramError::InvalidAccountData) // Map SliceTryFromError to ProgramError
    };

    // Ensure src is long enough for the fixed-size parts
    if src.len() < 292 + 32 { // 292 for fields before curve_parameters, 32 for curve_parameters
        return Err(ProgramError::InvalidAccountData);
    }

    let version = src[0];
    let is_initialized = src[1] != 0;
    let bump_seed = src[2];
    let pool_token_program_id = to_pubkey(&src[3..35])?;
    let token_account_a = to_pubkey(&src[35..67])?;
    let token_account_b = to_pubkey(&src[67..99])?;
    let token_pool = to_pubkey(&src[99..131])?;
    let mint_a = to_pubkey(&src[131..163])?;
    let mint_b = to_pubkey(&src[163..195])?;
    let fee_account = to_pubkey(&src[195..227])?;

    let trade_fee_numerator = to_u64(&src[227..235])?;
    let trade_fee_denominator = to_u64(&src[235..243])?;
    let owner_trade_fee_numerator = to_u64(&src[243..251])?;
    let owner_trade_fee_denominator = to_u64(&src[251..259])?;
    let owner_withdraw_fee_numerator = to_u64(&src[259..267])?;
    let owner_withdraw_fee_denominator = to_u64(&src[267..275])?;
    let host_fee_numerator = to_u64(&src[275..283])?;
    let host_fee_denominator = to_u64(&src[283..291])?;

    let curve_type = src[291];
    let mut curve_parameters = [0u8; 32];
    curve_parameters.copy_from_slice(&src[292..292+32]); // Ensure correct slicing for curve_parameters

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
