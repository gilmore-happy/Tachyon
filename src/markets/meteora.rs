use crate::arbitrage::types::{Route, TokenInfos};
use crate::common::constants::Env;
use crate::common::debug::print_json_segment;
use crate::common::utils::{from_pubkey, from_str, make_request};
use crate::markets::types::{Dex, DexLabel, Market, PoolItem, SimulationRes}; // Removed SimulationError
use crate::markets::utils::to_pair_string;

use anyhow::Result;
use borsh::{BorshDeserialize, BorshSerialize};
use log::{error, info};
use reqwest::get;
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use solana_account_decoder::UiAccountEncoding;
// Removed unused solana_client::rpc_client::RpcClient
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType};
use solana_program::pubkey::Pubkey;
use std::collections::HashMap;
// use std::fs; // Replaced with tokio::fs
// use std::fs::File; // Replaced with tokio::fs::File
// use std::io::{BufWriter, Write}; // Write is part of tokio::io::AsyncWriteExt, BufWriter might need tokio::io::BufWriter
use tokio::fs; // Added
use tokio::fs::File; // Added
use tokio::io::{AsyncWriteExt, BufWriter}; // Added

use crate::markets::errors::MarketSimulationError; // Added
use log::warn; // Added for logging fallback

#[derive(Debug)]
pub struct MeteoraDEX {
    pub dex: Dex,
    pub pools: Vec<PoolItem>,
}
impl MeteoraDEX {
    pub async fn new(mut dex: Dex) -> Self { // Changed to async
        let mut pools_vec = Vec::new();

        let data = fs::read_to_string("src/markets/cache/meteora-markets.json")
            .await // Changed to await
            .expect("Error reading file");
        let json_value: Root = serde_json::from_str(&data).unwrap();

        for pool in json_value.clone() {
            //Serialization foraccount_data
            let mut serialized_data: Vec<u8> = Vec::new();
            BorshSerialize::serialize(&pool, &mut serialized_data).unwrap();
            let fee: f64 = pool.max_fee_percentage.parse().unwrap();
            let liquidity: f64 = pool.liquidity.parse().unwrap();
            let item: PoolItem = PoolItem {
                mint_a: pool.mint_x.clone(),
                mint_b: pool.mint_y.clone(),
                vault_a: pool.reserve_x.clone(),
                vault_b: pool.reserve_y.clone(),
                trade_fee_rate: fee as u128,
            };
            pools_vec.push(item);

            let market: Market = Market {
                token_mint_a: pool.mint_x.clone(),
                token_vault_a: pool.reserve_x.clone(),
                token_mint_b: pool.mint_y.clone(),
                token_vault_b: pool.reserve_y.clone(),
                dex_label: DexLabel::Meteora,
                fee: fee as u64,
                id: pool.address.clone(),
                account_data: Some(serialized_data),
                liquidity: Some(liquidity as u64),
            };

            let pair_string = to_pair_string(pool.mint_x, pool.mint_y);
            if dex.pair_to_markets.contains_key(&pair_string.clone()) {
                let vec_market = dex.pair_to_markets.get_mut(&pair_string).unwrap();
                vec_market.push(market);
            } else {
                dex.pair_to_markets.insert(pair_string, vec![market]);
            }
        }

        info!("Meteora : {} pools founded", json_value.len());
        Self {
            dex,
            pools: pools_vec,
        }
    }
}

pub async fn fetch_data_meteora() -> Result<(), Box<dyn std::error::Error>> {
    let response = get("https://dlmm-api.meteora.ag/pair/all").await?;
    // info!("response: {:?}", response);
    // info!("response-status: {:?}", response.status().is_success());
    if response.status().is_success() {
        let data = response.text().await?;

        match serde_json::from_str::<Root>(&data) {
            Ok(json) => {
                let file = File::create("src/markets/cache/meteora-markets.json").await?; // Changed to tokio::fs::File and await
                let mut writer = BufWriter::new(file);
                writer.write_all(serde_json::to_string(&json)?.as_bytes()).await?; // Changed to await
                writer.flush().await?; // Changed to await
                info!("Data written to 'meteora-markets.json' successfully.");
            }
            Err(e) => {
                eprintln!("Failed to deserialize JSON: {:?}", e);
                // Optionally, save the raw JSON data to inspect it manually
                // let mut raw_file = File::create("src/markets/cache/meteora-markets-raw.json")?;
                // let mut writer = BufWriter::new(raw_file);
                // writer.write_all(data.as_bytes())?;
                // writer.flush()?;
                let _result = print_json_segment(
                    "src/markets/cache/meteora-markets-raw.json",
                    3426919 - 100 as u64,
                    2000,
                );
                // raw_file.write_all(data.as_bytes())?;
                // info!("Raw data written to 'meteora-markets-raw.json' for inspection.");
            }
        }
    } else {
        error!(
            "Fetch of 'meteora-markets.json' not successful: {}",
            response.status()
        );
    }
    Ok(())
}

pub async fn fetch_new_meteora_pools(
    rpc_client: &solana_client::nonblocking::rpc_client::RpcClient, // Changed type
    token: String,
    on_tokena: bool,
) -> Vec<(Pubkey, Market)> {
    let meteora_program = "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo".to_string();
    // let pool = "5nRheYVXMTHEJXyAYG9KsUsXDTzvj9Las8M6NfNojaR".to_string();
    // println!("DEBUG ---- Token: {:?}", token);

    let mut new_markets: Vec<(Pubkey, Market)> = Vec::new();
    let filters = Some(vec![
        RpcFilterType::Memcmp(Memcmp::new(
            if on_tokena { 88 } else { 120 },
            MemcmpEncodedBytes::Base58(token.clone()),
        )),
        RpcFilterType::DataSize(904),
    ]);

    let accounts = rpc_client
        .get_program_accounts_with_config(
            &from_str(&meteora_program).unwrap(),
            RpcProgramAccountsConfig {
                filters,
                account_config: RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    commitment: Some(rpc_client.commitment()),
                    ..RpcAccountInfoConfig::default()
                },
                ..RpcProgramAccountsConfig::default()
            },
        )
        .await // Changed to await
        .unwrap();

    for account in accounts.clone() {
        // println!("Address: {:?}", &account.0);
        // println!("account data: {:?}", &account.1.data);
        let meteora_market = AccountData::try_from_slice(&account.1.data).unwrap();
        // println!("meteora_market: {:?}", meteora_market);
        let market: Market = Market {
            token_mint_a: from_pubkey(meteora_market.token_xmint),
            token_vault_a: from_pubkey(meteora_market.reserve_x),
            token_mint_b: from_pubkey(meteora_market.token_ymint),
            token_vault_b: from_pubkey(meteora_market.reserve_y),
            dex_label: DexLabel::Meteora,
            fee: 0_u64,
            id: from_pubkey(account.0).clone(),
            account_data: Some(account.1.data),
            liquidity: Some(666_u64),
        };
        new_markets.push((account.0, market));
    }
    // println!("Accounts: {:?}", accounts);
    // println!("new_markets: {:?}", new_markets);
    new_markets
}

// Simulate one route
// I want to get the data of the market i'm interested in this route
pub async fn simulate_route_meteora(
    printing_amt: bool,
    amount_in: u64,
    route: Route,
    market: Market,
    tokens_infos: HashMap<String, TokenInfos>,
) -> Result<(u64, u64), MarketSimulationError> { // Changed return type
    // println!("account_data: {:?}", &market.account_data.clone().unwrap());
    // println!("market: {:?}", market.clone());
    // let meteora_data = AccountData::try_from_slice(&market.account_data.expect("Account data problem // METEORA")).expect("Account data not fit bytes length");

    let token0 = tokens_infos.get(&market.token_mint_a).unwrap();
    let token1 = tokens_infos.get(&market.token_mint_b).unwrap();

    let amount_in_uint = amount_in as u64;

    let params = format!(
        "poolId={}&token0to1={}&amountIn={}&tokenInSymbol={}&tokenOutSymbol={}",
        market.id,
        route.token_0to1,
        amount_in_uint,
        if route.token_0to1 {
            &token0.symbol
        } else {
            &token1.symbol
        },
        if route.token_0to1 {
            &token1.symbol
        } else {
            &token0.symbol
        },
    );

    // Simulate a swap
    let env = Env::new();
    let domain = env.simulator_url;

    let req_url = format!("{}meteora_quote?{}", domain, params);
    
    let res = make_request(req_url).await.map_err(|e| MarketSimulationError::ApiRequestFailed {
        market: "LocalSimulator-MeteoraQuote".to_string(),
        message: e.to_string(),
        source: Some(Box::new(e)),
    })?;
    let res_text = res.text().await.map_err(|e| MarketSimulationError::ApiRequestFailed {
        market: "LocalSimulator-MeteoraQuote".to_string(),
        message: format!("Failed to read response text: {}", e),
        source: Some(Box::new(e)),
    })?;

    if let Ok(json_value) = serde_json::from_str::<SimulationRes>(&res_text) {
        if printing_amt {
            info!(
                "LocalSim Meteora: In: {} {}, EstOut: {} {}, EstMinOut: {} {}",
                json_value.amount_in, 
                if route.token_0to1 { token0.symbol.clone() } else { token1.symbol.clone() },
                json_value.estimated_amount_out, 
                if route.token_0to1 { token1.symbol.clone() } else { token0.symbol.clone() },
                json_value.estimated_min_amount_out.clone().unwrap_or_default(),
                if route.token_0to1 { token1.symbol.clone() } else { token0.symbol.clone() }
            );
        }

        let estimated_out = json_value.estimated_amount_out.parse::<u64>()
            .map_err(|e| MarketSimulationError::AmountParseError {
                market: "LocalSimulator-MeteoraQuote".to_string(),
                value: json_value.estimated_amount_out.clone(),
                field: "estimated_amount_out".to_string(),
                source: e,
            })?;
        
        let min_out_str = json_value.estimated_min_amount_out.unwrap_or_else(|| {
            warn!("LocalSimulator-MeteoraQuote: estimated_min_amount_out is None for route {}, pool {}. Falling back to estimated_amount_out.", route.id, route.pool_address);
            json_value.estimated_amount_out.clone()
        });

        let min_out = min_out_str.parse::<u64>()
            .map_err(|e| MarketSimulationError::AmountParseError {
                market: "LocalSimulator-MeteoraQuote".to_string(),
                value: min_out_str,
                field: "estimated_min_amount_out (or fallback)".to_string(),
                source: e,
            })?;

        return Ok((estimated_out, min_out));
    } else if let Ok(error_value) = serde_json::from_str::<super::types::SimulationError>(&res_text) {
        Err(MarketSimulationError::ApiRequestFailed {
            market: "LocalSimulator-MeteoraQuote".to_string(),
            message: error_value.error,
            source: None,
        })
    } else {
        Err(MarketSimulationError::InvalidResponseFormat {
            market: "LocalSimulator-MeteoraQuote".to_string(),
            details: format!("Unexpected response format: {}", res_text),
        })
    }
}

fn de_rating<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::String(s) => s.parse().map_err(de::Error::custom)?,
        Value::Number(num) => num.as_f64().ok_or(de::Error::custom("Invalid number"))? as f64,
        Value::Null => 0.0,
        _ => return Err(de::Error::custom("wrong type")),
    })
}

pub type Root = Vec<MeteoraPool>;

#[derive(
    Default, BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct MeteoraPool2 {
    pub address: String,
    #[serde(deserialize_with = "de_rating")]
    pub apr: f64,
    #[serde(deserialize_with = "de_rating")]
    pub apy: f64,
    pub base_fee_percentage: String,
    #[serde(deserialize_with = "de_rating")]
    pub bin_step: f64,
    pub cumulative_fee_volume: String,
    pub cumulative_trade_volume: String,
    #[serde(deserialize_with = "de_rating")]
    pub current_price: f64,
    #[serde(deserialize_with = "de_rating")]
    pub farm_apr: f64,
    #[serde(deserialize_with = "de_rating")]
    pub farm_apy: f64,
    #[serde(deserialize_with = "de_rating")]
    pub fees_24h: f64,
    pub hide: bool,
    pub liquidity: String,
    pub max_fee_percentage: String,
    pub mint_x: String,
    pub mint_y: String,
    pub name: String,
    pub protocol_fee_percentage: String,
    pub reserve_x: String,
    #[serde(deserialize_with = "de_rating")]
    pub reserve_x_amount: f64,
    pub reserve_y: String,
    #[serde(deserialize_with = "de_rating")]
    pub reserve_y_amount: f64,
    pub reward_mint_x: String,
    pub reward_mint_y: String,
    #[serde(deserialize_with = "de_rating")]
    pub today_fees: f64,
    #[serde(deserialize_with = "de_rating")]
    pub trade_volume_24h: f64,
}

#[derive(
    Default, BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct MeteoraPool {
    pub address: String,
    pub name: String,
    #[serde(rename = "mint_x")]
    pub mint_x: String,
    #[serde(rename = "mint_y")]
    pub mint_y: String,
    #[serde(rename = "reserve_x")]
    pub reserve_x: String,
    #[serde(rename = "reserve_y")]
    pub reserve_y: String,
    #[serde(rename = "reserve_x_amount")]
    pub reserve_x_amount: i128,
    #[serde(rename = "reserve_y_amount")]
    pub reserve_y_amount: i128,
    #[serde(rename = "bin_step")]
    pub bin_step: i64,
    #[serde(rename = "base_fee_percentage")]
    pub base_fee_percentage: String,
    #[serde(rename = "max_fee_percentage")]
    pub max_fee_percentage: String,
    #[serde(rename = "protocol_fee_percentage")]
    pub protocol_fee_percentage: String,
    pub liquidity: String,
    #[serde(rename = "reward_mint_x")]
    pub reward_mint_x: String,
    #[serde(rename = "reward_mint_y")]
    pub reward_mint_y: String,
    #[serde(deserialize_with = "de_rating", rename = "fees_24h")]
    pub fees_24h: f64,
    #[serde(deserialize_with = "de_rating", rename = "today_fees")]
    pub today_fees: f64,
    #[serde(deserialize_with = "de_rating", rename = "trade_volume_24h")]
    pub trade_volume_24h: f64,
    #[serde(rename = "cumulative_trade_volume")]
    pub cumulative_trade_volume: String,
    #[serde(rename = "cumulative_fee_volume")]
    pub cumulative_fee_volume: String,
    #[serde(deserialize_with = "de_rating", rename = "current_price")]
    pub current_price: f64,
    #[serde(deserialize_with = "de_rating")]
    pub apr: f64,
    #[serde(deserialize_with = "de_rating")]
    pub apy: f64,
    #[serde(deserialize_with = "de_rating", rename = "farm_apr")]
    pub farm_apr: f64,
    #[serde(deserialize_with = "de_rating", rename = "farm_apy")]
    pub farm_apy: f64,
    pub hide: bool,
}

///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////         ACCOUNT DATA            ///////////////////////////////////////////////////////////////////////////
///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(
    Default, BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct AccountData {
    pub offset: u64, //Probably the signature of the account.data
    pub parameters: StaticParameters,
    pub v_parameters: VParameters,
    pub bump_seed: [u8; 1],
    pub bin_step_seed: [u8; 2],
    pub pair_type: u8,
    pub active_id: i32,
    pub bin_step: u16,
    pub status: u8,
    pub padding1: [u8; 5],
    #[serde(rename = "tokenXMint")]
    pub token_xmint: Pubkey,
    #[serde(rename = "tokenYMint")]
    pub token_ymint: Pubkey,
    pub reserve_x: Pubkey,
    pub reserve_y: Pubkey,
    pub protocol_fee: ProtocolFee,
    pub fee_owner: Pubkey,
    pub reward_infos: [RewardInfo; 2],
    pub oracle: Pubkey,
    pub bin_array_bitmap: [u64; 16],
    pub last_updated_at: i64,
    pub whitelisted_wallet: [Pubkey; 2],
    pub base_key: Pubkey,
    pub activation_slot: u64,
    pub swap_cap_deactivate_slot: u64,
    pub max_swapped_amount: u64,
    pub lock_durations_in_slot: u64,
    pub creator: Pubkey,
    pub reserved: [u8; 24],
}

#[derive(
    Default, BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct StaticParameters {
    pub base_factor: u16,
    pub filter_period: u16,
    pub decay_period: u16,
    pub reduction_factor: u16,
    pub variable_fee_control: u32,
    pub max_volatility_accumulator: u32,
    pub min_bin_id: i32,
    pub max_bin_id: i32,
    pub protocol_share: u16,
    pub padding: [u8; 6],
}

#[derive(
    Default, BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct VParameters {
    pub volatility_accumulator: u32,
    pub volatility_reference: u32,
    pub index_reference: i32,
    pub padding: [u8; 4],
    pub last_update_timestamp: i64,
    pub padding1: [u8; 8],
}

#[derive(
    Default, BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolFee {
    pub amount_x: u64,
    pub amount_y: u64,
}

#[derive(
    Default, BorshDeserialize, BorshSerialize, Debug, Clone, PartialEq, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct RewardInfo {
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub funder: Pubkey,
    pub reward_duration: u64,
    pub reward_duration_end: u64,
    pub reward_rate: u128,
    pub last_update_time: u64,
    pub cumulative_seconds_with_empty_liquidity_reward: u64,
}
