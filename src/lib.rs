pub mod arbitrage;
pub mod common;
pub mod data;
pub mod execution;
pub mod fees;
pub mod markets;
pub mod strategies;
pub mod transactions; // Add the fees module
pub mod telemetry;

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;
    use crate::arbitrage::types::{TokenInArb, SwapPathResult, SwapRouteSimulation};
    use crate::markets::types::DexLabel;
    use crate::transactions::create_transaction::{
        write_lut_for_market, 
        create_ata_extendlut_transaction, 
        ChainType, 
        SendOrSimulate
    };
    use crate::common::utils::from_str;

    #[test]
    fn write_in_write_lut_for_market() {
        let market: Pubkey = Pubkey::new_unique();
        let lut_address: Pubkey = Pubkey::new_unique();
        let _ = write_lut_for_market(market, lut_address, true);
        let market2: Pubkey = Pubkey::new_unique();
        let lut_address2: Pubkey = Pubkey::new_unique();
        let _ = write_lut_for_market(market2, lut_address2, true);
    }
    
    #[tokio::test]
    async fn test_devnet_create_ata_extendlut_transaction() {
        let tokens_to_arb: Vec<TokenInArb> = vec![
            TokenInArb {
                token: String::from("So11111111111111111111111111111111111111112"),
                symbol: String::from("SOL"),
                decimals: 9,
            }, // Base token here
            TokenInArb {
                token: String::from("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU"),
                symbol: String::from("USDC"),
                decimals: 6,
            },
            // TokenInArb{token: String::from("9jaZhJM6nMHTo4hY9DGabQ1HNuUWhJtm7js1fmKMVpkN"), symbol: String::from("AMC"), decimals: 6},
        ];

        let spr = SwapPathResult {
            path_id: 1,
            hops: 2,
            tokens_path: "SOL-AMC-GME-SOL".to_string(),
            route_simulations: vec![
                SwapRouteSimulation {
                    id_route: 17,
                    pool_address: "HZZofxusqKaA9JqaeXW8PtUALRXUwSLLwnt4eBFiyEdC".to_string(),
                    dex_label: DexLabel::Raydium,
                    token_0to1: false,
                    token_in: "So11111111111111111111111111111111111111112".to_string(),
                    token_out: "9jaZhJM6nMHTo4hY9DGabQ1HNuUWhJtm7js1fmKMVpkN".to_string(),
                    amount_in: 300000000,
                    estimated_amount_out: "8703355798604".to_string(),
                    minimum_amount_out: 8617183959013,
                },
                SwapRouteSimulation {
                    id_route: 26,
                    pool_address: "9kbAydmdxuqrJGvaCmmnJaGnaC96zAkBHZ9dQn3cm9PZ".to_string(),
                    dex_label: DexLabel::Meteora,
                    token_0to1: true,
                    token_in: "9jaZhJM6nMHTo4hY9DGabQ1HNuUWhJtm7js1fmKMVpkN".to_string(),
                    token_out: "8wXtPeU6557ETkp9WHFY1n1EcU6NxDvbAggHGsMYiHsB".to_string(),
                    amount_in: 8703355798604, // 0.001 SOL
                    estimated_amount_out: "4002500590682".to_string(),
                    minimum_amount_out: 3998498090091,
                },
                SwapRouteSimulation {
                    id_route: 13,
                    pool_address: "2qKjGUBdgLcGVt1JbjLfXtphPQNkq4ujd6PyrTBWkeJ5".to_string(),
                    dex_label: DexLabel::OrcaWhirlpools,
                    token_0to1: false,
                    token_in: "8wXtPeU6557ETkp9WHFY1n1EcU6NxDvbAggHGsMYiHsB".to_string(),
                    token_out: "So11111111111111111111111111111111111111112".to_string(),
                    amount_in: 4002500590682, // 0.001 SOL
                    estimated_amount_out: "300776562".to_string(),
                    minimum_amount_out: 297798576,
                },
            ],
            token_in: "So11111111111111111111111111111111111111112".to_string(),
            token_in_symbol: "SOL".to_string(),
            token_out: "So11111111111111111111111111111111111111112".to_string(),
            token_out_symbol: "SOL".to_string(),
            amount_in: 300000000,
            estimated_amount_out: "300776562".to_string(),
            estimated_min_amount_out: "297798576".to_string(),
            result: 776562.0,
        };

        let tokens: Vec<Pubkey> = tokens_to_arb
            .into_iter()
            .map(|tok| from_str(tok.token.as_str()).unwrap())
            .collect();
        let _ = create_ata_extendlut_transaction(
            ChainType::Devnet,
            SendOrSimulate::Send,
            spr.clone(),
            from_str("6nGymM5X1djYERKZtoZ3Yz3thChMVF6jVRDzhhcmxuee").unwrap(),
            tokens,
        )
        .await;
    }
}
