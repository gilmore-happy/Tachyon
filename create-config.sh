#!/bin/bash

# Script to create a new default configuration file for the Solana MEV Bot

echo "Creating new default configuration file..."

# Create the config.json file with default values
cat > config.json << EOF
{
  "tokens_to_trade": [
    {
      "address": "So11111111111111111111111111111111111111112",
      "symbol": "SOL"
    },
    {
      "address": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
      "symbol": "USDC"
    }
  ],
  "input_vectors": [
    {
      "tokens_to_arb": [
        {
          "address": "So11111111111111111111111111111111111111112",
          "symbol": "SOL"
        }
      ],
      "include_1hop": false,
      "include_2hop": false,
      "numbers_of_best_paths": 1,
      "get_fresh_pools_bool": false
    }
  ],
  "active_strategies": [
    "Massive",
    "BestPath"
  ],
  "simulation_amount": 3500000000,
  "min_profit_threshold": 20.0,
  "max_slippage": 0.02,
  "execution_mode": "Simulate",
  "fetch_new_pools": false,
  "restrict_sol_usdc": true,
  "path_best_strategie": "best_paths_selected/ultra_strategies/0-SOL-SOLLY-1-SOL-SPIKE-2-SOL-AMC-GME.json",
  "optimism_path": "optimism_transactions/11-6-2024-SOL-SOLLY-SOL-0.json",
  "fee_mode": "ProfitBased",
  "fee_cache_duration_secs": 2,
  "cache_dir": "src/markets/cache",
  "output_dir": "best_paths_selected"
}
EOF

echo "Configuration file created: config.json"
echo ""
echo "Available Strategies: Massive, BestPath, Optimism, All"
echo "Available Execution Modes: Simulate, Paper, Live"
echo "Available Fee Modes: Conservative, ProfitBased, Aggressive"
echo ""
echo "Edit the file manually to customize your bot's behavior."