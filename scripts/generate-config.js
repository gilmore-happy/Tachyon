#!/usr/bin/env node

/**
 * Solana MEV Bot Configuration Generator
 * 
 * This script helps users create a custom config.json file for the bot
 * with proper validation of addresses, strategy options, etc.
 */

const fs = require('fs');
const path = require('path');
const readline = require('readline');

const rl = readline.createInterface({
  input: process.stdin,
  output: process.stdout
});

// Default config template
const defaultConfig = {
  tokens_to_trade: [
    {
      address: "So11111111111111111111111111111111111111112",
      symbol: "SOL"
    }
  ],
  input_vectors: [
    {
      tokens_to_arb: [
        {
          address: "So11111111111111111111111111111111111111112",
          symbol: "SOL"
        }
      ],
      include_1hop: false,
      include_2hop: false,
      numbers_of_best_paths: 1,
      get_fresh_pools_bool: false
    }
  ],
  active_strategies: ["Massive", "BestPath"],
  simulation_amount: 3500000000,
  min_profit_threshold: 20.0,
  max_slippage: 0.02,
  execution_mode: "Simulate",
  fetch_new_pools: false,
  restrict_sol_usdc: true,
  path_best_strategie: "best_paths_selected/ultra_strategies/0-SOL-SOLLY-1-SOL-SPIKE-2-SOL-AMC-GME.json",
  optimism_path: "optimism_transactions/11-6-2024-SOL-SOLLY-SOL-0.json",
  fee_mode: "ProfitBased",
  fee_cache_duration_secs: 2,
  cache_dir: "src/markets/cache",
  output_dir: "best_paths_selected"
};

// Validate Solana addresses
function isValidSolanaAddress(address) {
  return /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(address);
}

// Ask user for configuration options
async function promptForConfig() {
  const config = JSON.parse(JSON.stringify(defaultConfig)); // Clone the default config
  
  console.log("=== Solana MEV Bot Configuration Generator ===");
  console.log("Press Enter to accept default values shown in brackets\n");
  
  // Select strategies
  const strategyOptions = ["Massive", "BestPath", "Optimism", "All"];
  console.log("Available strategies: " + strategyOptions.join(", "));
  const strategiesInput = await new Promise(resolve => {
    rl.question(`Enter strategies to use (comma-separated) [${config.active_strategies.join(", ")}]: `, answer => {
      resolve(answer || config.active_strategies.join(", "));
    });
  });
  
  config.active_strategies = strategiesInput.split(",").map(s => s.trim()).filter(s => strategyOptions.includes(s));
  
  // Execution mode
  const executionModes = ["Simulate", "Paper", "Live"];
  console.log("\nAvailable execution modes:");
  console.log("- Simulate: Only simulates transactions without sending them");
  console.log("- Paper: Simulates and logs transactions with real-time pricing");
  console.log("- Live: Actually executes transactions on-chain");
  
  const executionModeInput = await new Promise(resolve => {
    rl.question(`Select execution mode [${config.execution_mode}]: `, answer => {
      resolve(answer || config.execution_mode);
    });
  });
  
  if (executionModes.includes(executionModeInput)) {
    config.execution_mode = executionModeInput;
  }
  
  // Simulation amount
  const simulationAmountInput = await new Promise(resolve => {
    rl.question(`Simulation amount in lamports [${config.simulation_amount}]: `, answer => {
      resolve(answer || config.simulation_amount);
    });
  });
  
  config.simulation_amount = parseInt(simulationAmountInput, 10) || config.simulation_amount;
  
  // Min profit threshold
  const profitThresholdInput = await new Promise(resolve => {
    rl.question(`Minimum profit threshold in USD [${config.min_profit_threshold}]: `, answer => {
      resolve(answer || config.min_profit_threshold);
    });
  });
  
  config.min_profit_threshold = parseFloat(profitThresholdInput) || config.min_profit_threshold;
  
  // Max slippage
  const slippageInput = await new Promise(resolve => {
    rl.question(`Maximum slippage (as decimal, e.g. 0.02 = 2%) [${config.max_slippage}]: `, answer => {
      resolve(answer || config.max_slippage);
    });
  });
  
  config.max_slippage = parseFloat(slippageInput) || config.max_slippage;
  
  // Fee mode
  const feeModes = ["Conservative", "ProfitBased", "Aggressive"];
  console.log("\nAvailable fee modes:");
  console.log("- Conservative: Lower fees, safer execution");
  console.log("- ProfitBased: Adjusts fees based on expected profit");
  console.log("- Aggressive: Higher fees to ensure execution");
  
  const feeModeInput = await new Promise(resolve => {
    rl.question(`Select fee mode [${config.fee_mode}]: `, answer => {
      resolve(answer || config.fee_mode);
    });
  });
  
  if (feeModes.includes(feeModeInput)) {
    config.fee_mode = feeModeInput;
  }
  
  // Add additional tokens?
  const addTokensInput = await new Promise(resolve => {
    rl.question(`Would you like to add additional tokens to trade? (yes/no) [no]: `, answer => {
      resolve(answer.toLowerCase() === "yes");
    });
  });
  
  if (addTokensInput) {
    let addingTokens = true;
    while (addingTokens) {
      const tokenAddress = await new Promise(resolve => {
        rl.question("Enter token address: ", answer => {
          resolve(answer);
        });
      });
      
      if (!isValidSolanaAddress(tokenAddress)) {
        console.log("Invalid Solana address format. Skipping.");
        continue;
      }
      
      const tokenSymbol = await new Promise(resolve => {
        rl.question("Enter token symbol: ", answer => {
          resolve(answer);
        });
      });
      
      config.tokens_to_trade.push({
        address: tokenAddress,
        symbol: tokenSymbol.toUpperCase()
      });
      
      const continueInput = await new Promise(resolve => {
        rl.question("Add another token? (yes/no) [no]: ", answer => {
          resolve(answer.toLowerCase() === "yes");
        });
      });
      
      addingTokens = continueInput;
    }
  }
  
  return config;
}

// Write configuration to file
async function main() {
  try {
    const config = await promptForConfig();
    
    const configPath = path.join(process.cwd(), 'config.json');
    fs.writeFileSync(configPath, JSON.stringify(config, null, 2));
    
    console.log(`\nConfiguration saved to ${configPath}`);
    console.log("You can edit this file directly to make further changes.");
    
  } catch (error) {
    console.error("Error generating configuration:", error);
  } finally {
    rl.close();
  }
}

main();