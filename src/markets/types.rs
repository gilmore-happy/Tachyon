use crate::markets::utils::to_pair_string;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use strum_macros::{EnumIter, Display}; // Added Display

#[derive(Debug, Clone, EnumIter, Serialize, Deserialize, Eq, PartialEq, Hash, Display)] // Added Display
pub enum DexLabel {
    Orca,
    OrcaWhirlpools,
    Raydium,
    RaydiumClmm,
    Meteora,
}

impl DexLabel {
    pub fn str(&self) -> String {
        match self {
            DexLabel::Orca => String::from("Orca"),
            DexLabel::OrcaWhirlpools => String::from("Orca (Whirlpools)"),
            DexLabel::Raydium => String::from("Raydium"),
            DexLabel::RaydiumClmm => String::from("Raydium CLMM"),
            DexLabel::Meteora => String::from("Meteora"),
        }
    }
    pub fn api_url(&self) -> String {
        match self {
            DexLabel::Orca => String::from("https://api.orca.so/allPools"),
            DexLabel::OrcaWhirlpools => {
                String::from("https://api.mainnet.orca.so/v1/whirlpool/list")
            }
            DexLabel::Raydium => String::from("https://api.raydium.io/v2/main/pairs"),
            DexLabel::RaydiumClmm => String::from("https://api.raydium.io/v2/ammV3/ammPools"),
            DexLabel::Meteora => String::from("https://dlmm-api.meteora.ag/pair/all"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    #[serde(alias = "tokenMintA")]
    pub token_mint_a: String,
    #[serde(alias = "tokenVaultA")]
    pub token_vault_a: String,
    #[serde(alias = "tokenMintB")]
    pub token_mint_b: String,
    #[serde(alias = "tokenVaultB")]
    pub token_vault_b: String,
    #[serde(alias = "dexLabel")]
    pub dex_label: DexLabel,
    pub fee: u64,
    pub id: String,
    pub account_data: Option<Vec<u8>>,
    pub liquidity: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Dex {
    pub pair_to_markets: HashMap<String, Vec<Market>>,
    // ammCalcAddPoolMessages: AmmCalcWorkerParamMessage[];
    pub label: DexLabel,
}

impl Dex {
    pub fn new(label: DexLabel) -> Self {
        let pair_to_markets = HashMap::new();
        Dex {
            pair_to_markets,
            label,
        }
    }

    // getAmmCalcAddPoolMessages(): AmmCalcWorkerParamMessage[] {
    //   return this.ammCalcAddPoolMessages;
    // }

    pub fn get_markets_for_pair(&self, mint_a: String, mint_b: String) -> &Vec<Market> {
        let pair = to_pair_string(mint_a, mint_b);
        self.pair_to_markets.get(&pair).unwrap()
    }

    pub fn get_all_markets(&self) -> Vec<&Vec<Market>> {
        let mut all_markets = Vec::new();

        for markets in self.pair_to_markets.values() {
            all_markets.push(markets);
        }
        all_markets
    }
}

#[derive(Debug)]
pub struct PoolItem {
    pub mint_a: String,
    pub mint_b: String,
    pub vault_a: String,
    pub vault_b: String,
    pub trade_fee_rate: u128,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SimulationRes {
    #[serde(alias = "amountIn")]
    pub amount_in: String,
    #[serde(alias = "estimatedAmountOut")]
    pub estimated_amount_out: String,
    #[serde(alias = "estimatedMinAmountOut")]
    pub estimated_min_amount_out: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct SimulationError {
    pub error: String,
}
