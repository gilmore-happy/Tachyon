use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;

use log::{debug, info};

use crate::arbitrage::types::{Route, SwapPath, TokenInArb, TokenInfos};
use crate::markets::types::{Dex, DexLabel, Market};

/// Represents a heuristic score for a potential arbitrage path
#[derive(Debug, Clone)]
pub struct PathScore {
    pub path_id: Vec<u32>,
    pub score: f64,
    pub path: SwapPath,
}

/// Implementation for priority queue ordering
impl Ord for PathScore {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare scores for binary heap (reverse order for max-heap)
        self.score.partial_cmp(&other.score)
            .unwrap_or(Ordering::Equal)
            .reverse()
    }
}

impl PartialOrd for PathScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for PathScore {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}

impl Eq for PathScore {}

/// Fast path evaluator that uses heuristics to prioritize paths
pub struct PathEvaluator {
    /// Map of market ID to liquidity 
    market_liquidity: HashMap<String, u64>,
    /// Map of token pair to historical success rate
    token_pair_success: HashMap<(String, String), f64>,
    /// Map of DEX to success rate
    dex_success_rate: HashMap<DexLabel, f64>,
    /// Minimum required liquidity to consider a path
    min_liquidity: u64,
    /// Transaction cost estimate in lamports
    tx_cost_estimate: u64,
}

impl PathEvaluator {
    /// Create a new path evaluator
    pub fn new(min_liquidity: u64, tx_cost_estimate: u64) -> Self {
        Self {
            market_liquidity: HashMap::new(),
            token_pair_success: HashMap::new(),
            dex_success_rate: HashMap::new(),
            min_liquidity,
            tx_cost_estimate,
        }
    }

    /// Update market liquidity information
    pub fn update_market_liquidity(&mut self, market_id: String, liquidity: u64) {
        self.market_liquidity.insert(market_id, liquidity);
    }

    /// Update token pair success rate
    pub fn update_token_pair_success(&mut self, token_a: String, token_b: String, success_rate: f64) {
        let key = if token_a < token_b {
            (token_a, token_b)
        } else {
            (token_b, token_a)
        };
        self.token_pair_success.insert(key, success_rate);
    }

    /// Update DEX success rate
    pub fn update_dex_success_rate(&mut self, dex: DexLabel, success_rate: f64) {
        self.dex_success_rate.insert(dex, success_rate);
    }

    /// Quickly filter paths that are unlikely to be profitable
    pub fn filter_paths(&self, paths: &[SwapPath], markets: &HashMap<String, Market>) -> Vec<SwapPath> {
        let mut filtered_paths = Vec::new();
        
        'outer: for path in paths {
            // Check if the path has at least one route
            if path.paths.is_empty() {
                continue;
            }
            
            // Skip paths with missing markets
            for route in &path.paths {
                if !markets.contains_key(&route.pool_address) {
                    continue 'outer;
                }
                
                // Skip paths with insufficient liquidity
                if let Some(liquidity) = self.market_liquidity.get(&route.pool_address) {
                    if *liquidity < self.min_liquidity {
                        continue 'outer;
                    }
                }
            }
            
            // If we get here, the path passed all filters
            filtered_paths.push(path.clone());
        }
        
        filtered_paths
    }
    
    /// Score paths by potential profitability using heuristics
    pub fn score_paths(&self, 
                      paths: &[SwapPath], 
                      markets: &HashMap<String, Market>,
                      tokens_info: &HashMap<String, TokenInfos>,
                      base_token: &TokenInArb) -> BinaryHeap<PathScore> {
        let mut scored_paths = BinaryHeap::new();
        
        for path in paths {
            let score = self.calculate_path_score(path, markets, tokens_info, base_token);
            
            // Add to priority queue if score is positive
            if score > 0.0 {
                scored_paths.push(PathScore {
                    path_id: path.id_paths.clone(),
                    score,
                    path: path.clone(),
                });
            }
        }
        
        scored_paths
    }
    
    /// Calculate a heuristic score for a path
    fn calculate_path_score(&self, 
                          path: &SwapPath, 
                          markets: &HashMap<String, Market>,
                          tokens_info: &HashMap<String, TokenInfos>,
                          base_token: &TokenInArb) -> f64 {
        let mut score = 1.0;
        
        // Calculate score based on market liquidity
        for route in &path.paths {
            // Get market info
            if let Some(market) = markets.get(&route.pool_address) {
                // Add liquidity factor (more liquidity = less slippage = better score)
                if let Some(liquidity) = market.liquidity {
                    // Normalize liquidity score between 0.5 and 2.0
                    let liquidity_factor = (liquidity as f64 / self.min_liquidity as f64).min(4.0).max(0.5);
                    score *= liquidity_factor;
                } else {
                    // If liquidity is unknown, use a neutral factor
                    score *= 0.75;
                }
                
                // Add DEX reliability factor
                if let Some(dex_success) = self.dex_success_rate.get(&route.dex) {
                    score *= dex_success;
                }
            } else {
                // Missing market info is a negative factor
                score *= 0.5;
            }
            
            // Add token pair success factor
            let key = if route.token_in < route.token_out {
                (route.token_in.clone(), route.token_out.clone())
            } else {
                (route.token_out.clone(), route.token_in.clone())
            };
            
            if let Some(pair_success) = self.token_pair_success.get(&key) {
                score *= pair_success;
            }
        }
        
        // Adjust score based on path length
        // Shorter paths (fewer hops) often have less slippage and gas costs
        match path.hops {
            1 => score *= 1.5, // Prefer direct swaps
            2 => score *= 1.2, // Two-hop paths are good
            _ => score *= 0.8, // Longer paths are more complex and risky
        }
        
        // Return final score, ensuring it's positive
        score.max(0.0)
    }
    
    /// Get a HashMap of all markets from a list of DEXes for fast lookup
    pub fn build_market_map(dexes: &[Dex]) -> HashMap<String, Market> {
        let mut market_map = HashMap::new();
        
        for dex in dexes {
            for markets_vec in dex.pair_to_markets.values() {
                for market in markets_vec {
                    market_map.insert(market.id.clone(), market.clone());
                }
            }
        }
        
        market_map
    }

    /// Prioritize paths for simulation based on heuristics
    pub fn prioritize_paths_for_simulation(
        &self,
        all_paths: &[SwapPath],
        dexes: &[Dex],
        tokens_info: &HashMap<String, TokenInfos>,
        base_token: &TokenInArb,
        max_paths: usize
    ) -> Vec<SwapPath> {
        // Build a fast lookup map for markets
        let market_map = Self::build_market_map(dexes);
        
        // First, quickly filter out obviously bad paths
        let filtered_paths = self.filter_paths(all_paths, &market_map);
        info!("Filtered down to {} paths from initial {} paths", 
              filtered_paths.len(), all_paths.len());
        
        // Score and prioritize remaining paths
        let scored_paths = self.score_paths(&filtered_paths, &market_map, tokens_info, base_token);
        
        // Take top N paths for simulation
        let mut prioritized_paths = Vec::new();
        for path_score in scored_paths.into_sorted_vec().into_iter().take(max_paths) {
            prioritized_paths.push(path_score.path);
        }
        
        info!("Selected top {} paths for simulation", prioritized_paths.len());
        prioritized_paths
    }
}

/// Create a default path evaluator with reasonable settings
pub fn create_default_evaluator() -> PathEvaluator {
    let mut evaluator = PathEvaluator::new(1_000_000, 5_000); // 1 SOL min liquidity, 0.000005 SOL tx cost
    
    // Set default DEX success rates based on empirical data
    evaluator.update_dex_success_rate(DexLabel::Raydium, 0.95);
    evaluator.update_dex_success_rate(DexLabel::RaydiumClmm, 0.92);
    evaluator.update_dex_success_rate(DexLabel::Orca, 0.95);
    evaluator.update_dex_success_rate(DexLabel::OrcaWhirlpools, 0.9);
    evaluator.update_dex_success_rate(DexLabel::Meteora, 0.85);
    
    evaluator
}