use crate::{
    common::{constants::Env, utils::from_str},
    markets::types::Market,
};
use anyhow::{anyhow, Result};
use log::{info, warn};
use solana_client::nonblocking::rpc_client::RpcClient; // Changed
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

//Get fresh data on all acounts with getMultipleAccounts
pub async fn get_fresh_accounts_states(
    mut accounts: HashMap<String, Market>,
) -> Result<HashMap<String, Market>, anyhow::Error> {
    let env = Env::new();
    let rpc_client = RpcClient::new(env.rpc_url); // Changed to nonblocking
    let mut counter_fresh_markets = 0;

    let mut markets_vec: Vec<Market> = Vec::new();
    let mut key_vec: Vec<String> = Vec::new();
    let mut pubkeys_vec: Vec<Pubkey> = Vec::new();
    for (key, market) in accounts.clone().iter() {
        markets_vec.push(market.clone());
        key_vec.push(key.clone());
        pubkeys_vec.push(from_str(&market.id)
            .map_err(|e| anyhow::anyhow!("Invalid pubkey {}: {}", market.id, e))?);
    }

    for i in (0..pubkeys_vec.len()).step_by(100) {
        let max_length = std::cmp::min(i + 100, pubkeys_vec.len());
        let batch = &pubkeys_vec[i..max_length];

        let batch_results = rpc_client.get_multiple_accounts(batch).await
            .map_err(|e| anyhow::anyhow!("Failed to get multiple accounts: {}", e))?;
        // println!("BatchResult {:?}", batch_results);
        for (j, account) in batch_results.iter().enumerate() {
            let account = match account {
                Some(acc) => acc.clone(),
                None => {
                    warn!("Account at index {} is None, skipping", j);
                    continue;
                }
            };
            // println!("WhirpoolAccount: {:?}", data);
            let account_data = account.data;

            markets_vec[j].account_data = Some(account_data);
            markets_vec[j].id = key_vec[j].clone();
            counter_fresh_markets += 1;
            accounts.insert(key_vec[j].clone(), markets_vec[j].clone());
        }
    }

    info!("💦💦 Fresh data for {:?} markets", counter_fresh_markets);
    Ok(accounts)
}
