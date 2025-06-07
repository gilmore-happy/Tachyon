// Script to fetch DEX market data using MCP servers
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');
const { promisify } = require('util');
const writeFileAsync = promisify(fs.writeFile);
const mkdirAsync = promisify(fs.mkdir);

// Define the cache directory
const CACHE_DIR = path.join(__dirname, '..', 'src', 'markets', 'cache');

// Make sure cache directory exists
async function ensureCacheDir() {
  try {
    await mkdirAsync(CACHE_DIR, { recursive: true });
  } catch (err) {
    if (err.code !== 'EEXIST') throw err;
  }
}

// Function to execute MCP commands
async function execMCP(server, method, params = {}) {
  try {
    console.log(`Executing ${server}.${method}...`);
    
    // Create the MCP command input
    const input = JSON.stringify({
      root: {
        name: method,
        params: params
      }
    });
    
    // Command to execute the MCP server
    const command = `claude-mcp-client ${server} <<'EOF'
${input}
EOF`;
    
    // Execute the command
    const result = execSync(command, { encoding: 'utf8' });
    
    // Parse the result
    const jsonResult = JSON.parse(result);
    
    if (jsonResult.error) {
      console.error(`Error from ${server}.${method}: ${jsonResult.error}`);
      return null;
    }
    
    return jsonResult;
  } catch (error) {
    console.error(`Error executing ${server}.${method}: ${error.message}`);
    return null;
  }
}

// Function to fetch Solana DEX protocols using solana MCP
async function fetchDexProtocols() {
  const result = await execMCP('solana', 'get_dex_protocols');
  
  if (!result || !result.protocols) {
    console.error('Failed to fetch DEX protocols');
    return null;
  }
  
  return result.protocols;
}

// Function to fetch DEX pools using dexpaprika MCP
async function fetchDexPools() {
  // First get the network information
  const networksResult = await execMCP('dexpaprika', 'getNetworks');
  
  if (!networksResult || !networksResult.networks) {
    console.error('Failed to fetch networks');
    return null;
  }
  
  // Find Solana network
  const solanaNetwork = networksResult.networks.find(n => 
    n.name.toLowerCase() === 'solana' || n.slug.toLowerCase() === 'solana'
  );
  
  if (!solanaNetwork) {
    console.error('Solana network not found');
    return null;
  }
  
  // Get DEXes on Solana
  const dexesResult = await execMCP('dexpaprika', 'getNetworkDexes', {
    networkId: solanaNetwork.id
  });
  
  if (!dexesResult || !dexesResult.dexes) {
    console.error('Failed to fetch Solana DEXes');
    return null;
  }
  
  // Find our target DEXes
  const targetDexes = {
    raydium: dexesResult.dexes.find(d => d.name.toLowerCase().includes('raydium')),
    orca: dexesResult.dexes.find(d => d.name.toLowerCase().includes('orca')),
    meteora: dexesResult.dexes.find(d => d.name.toLowerCase().includes('meteora'))
  };
  
  // Fetch pools for each DEX
  const poolsData = {};
  
  for (const [dexName, dex] of Object.entries(targetDexes)) {
    if (!dex) {
      console.log(`DEX ${dexName} not found in dexpaprika`);
      continue;
    }
    
    const poolsResult = await execMCP('dexpaprika', 'getDexPools', {
      dexId: dex.id,
      limit: 1000,
      offset: 0
    });
    
    if (!poolsResult || !poolsResult.pools) {
      console.error(`Failed to fetch pools for ${dexName}`);
      continue;
    }
    
    poolsData[dexName] = poolsResult.pools;
    console.log(`Fetched ${poolsResult.pools.length} pools for ${dexName}`);
  }
  
  return poolsData;
}

// Function to convert dexpaprika data to the format expected by the bot
function convertPoolsData(poolsData) {
  const result = {};
  
  // Convert Raydium pools
  if (poolsData.raydium) {
    result['raydium'] = poolsData.raydium.map(pool => ({
      name: `${pool.token0.symbol}/${pool.token1.symbol}`,
      amm_id: pool.address,
      lp_mint: pool.lpToken?.address || "",
      base_mint: pool.token0.address,
      quote_mint: pool.token1.address,
      market: pool.address,
      liquidity: pool.liquidity?.usd || 0,
      volume24h: pool.volume?.h24 || 0,
      volume24h_quote: pool.volume?.h24 || 0,
      fee24h: (pool.volume?.h24 || 0) * (pool.fee || 0.003) / 100,
      fee24h_quote: (pool.volume?.h24 || 0) * (pool.fee || 0.003) / 100,
      volume7d: (pool.volume?.h24 || 0) * 7,
      volume7d_quote: (pool.volume?.h24 || 0) * 7,
      fee7d: (pool.volume?.h24 || 0) * 7 * (pool.fee || 0.003) / 100,
      fee7d_quote: (pool.volume?.h24 || 0) * 7 * (pool.fee || 0.003) / 100,
      volume30d: (pool.volume?.h24 || 0) * 30,
      volume30d_quote: (pool.volume?.h24 || 0) * 30,
      fee30d: (pool.volume?.h24 || 0) * 30 * (pool.fee || 0.003) / 100,
      fee30d_quote: (pool.volume?.h24 || 0) * 30 * (pool.fee || 0.003) / 100,
      price: pool.token1Price || 0,
      lp_price: 0,
      token_amount_coin: pool.token0.reserve || 0,
      token_amount_pc: pool.token1.reserve || 0,
      token_amount_lp: 0,
      apr24h: pool.apy?.daily || 0,
      apr7d: pool.apy?.weekly || 0,
      apr30d: pool.apy?.monthly || 0
    }));
  } else {
    result['raydium'] = [];
  }
  
  // Convert Raydium CLMM pools (use same data with modifications)
  if (poolsData.raydium) {
    result['raydium-clmm'] = {
      data: poolsData.raydium.map(pool => ({
        id: pool.address,
        mint_a: pool.token0.address,
        mint_b: pool.token1.address,
        vault_a: "",  // Not available, would need additional fetching
        vault_b: "",  // Not available, would need additional fetching
        fee_rate: pool.fee || 0.003,
        amm_config: {
          trade_fee_rate: (pool.fee || 0.003) * 10000
        }
      }))
    };
  } else {
    result['raydium-clmm'] = { data: [] };
  }
  
  // Convert Orca pools
  result['orca'] = {};
  
  // Convert Orca Whirlpools
  if (poolsData.orca) {
    result['orca-whirpools'] = poolsData.orca.map(pool => ({
      address: pool.address,
      whirlpoolsConfig: "",
      whirlpoolData: {
        liquidity: pool.liquidity?.usd || 0,
        tickCurrentIndex: 0,
        sqrtPrice: "100000000",
        tickSpacing: 64,
        feeRate: pool.fee || 0.003
      },
      tokenA: {
        mint: pool.token0.address,
        symbol: pool.token0.symbol,
        decimals: pool.token0.decimals || 9
      },
      tokenB: {
        mint: pool.token1.address,
        symbol: pool.token1.symbol,
        decimals: pool.token1.decimals || 9
      }
    }));
  } else {
    result['orca-whirpools'] = [];
  }
  
  // Convert Meteora pools
  if (poolsData.meteora) {
    result['meteora'] = poolsData.meteora.map(pool => ({
      address: pool.address,
      bin_step: 1,
      reserve_x: "",  // Not available
      reserve_y: "",  // Not available
      mint_x: pool.token0.address,
      mint_y: pool.token1.address,
      liquidity: (pool.liquidity?.usd || 0).toString(),
      active_id: 0,
      max_fee_percentage: "0.3"
    }));
  } else {
    result['meteora'] = [];
  }
  
  return result;
}

// Function to fetch additional data using solana MCP
async function fetchArbitrageOpportunities() {
  const result = await execMCP('solana', 'analyze_arbitrage_opportunities');
  
  if (!result || !result.opportunities) {
    console.error('Failed to fetch arbitrage opportunities');
    return null;
  }
  
  return result.opportunities;
}

// Function to save data to cache files
async function saveToCache(data) {
  try {
    await ensureCacheDir();
    
    // Save Raydium data
    await writeFileAsync(
      path.join(CACHE_DIR, 'raydium-markets.json'),
      JSON.stringify(data['raydium'] || [], null, 2)
    );
    console.log('Saved Raydium data to cache');
    
    // Save Raydium CLMM data
    await writeFileAsync(
      path.join(CACHE_DIR, 'raydium-clmm-markets.json'),
      JSON.stringify(data['raydium-clmm'] || { data: [] }, null, 2)
    );
    console.log('Saved Raydium CLMM data to cache');
    
    // Save Orca data
    await writeFileAsync(
      path.join(CACHE_DIR, 'orca-markets.json'),
      JSON.stringify(data['orca'] || {}, null, 2)
    );
    console.log('Saved Orca data to cache');
    
    // Save Orca Whirlpools data
    await writeFileAsync(
      path.join(CACHE_DIR, 'orca-whirpools-markets.json'),
      JSON.stringify(data['orca-whirpools'] || [], null, 2)
    );
    console.log('Saved Orca Whirlpools data to cache');
    
    // Save Meteora data
    await writeFileAsync(
      path.join(CACHE_DIR, 'meteora-markets.json'),
      JSON.stringify(data['meteora'] || [], null, 2)
    );
    console.log('Saved Meteora data to cache');
    
  } catch (error) {
    console.error(`Error saving data to cache: ${error.message}`);
  }
}

// Main function
async function main() {
  try {
    console.log('Fetching DEX data using MCP servers...');
    
    // Fetch DEX protocols
    const protocols = await fetchDexProtocols();
    console.log(`Fetched ${protocols ? protocols.length : 0} DEX protocols`);
    
    // Fetch DEX pools
    const poolsData = await fetchDexPools();
    
    // Convert the data to the expected format
    const convertedData = convertPoolsData(poolsData);
    
    // Fetch arbitrage opportunities to enhance the data
    const opportunities = await fetchArbitrageOpportunities();
    if (opportunities) {
      console.log(`Fetched ${opportunities.length} arbitrage opportunities`);
    }
    
    // Save the data to cache files
    await saveToCache(convertedData);
    
    console.log('DEX data fetched and saved to cache files successfully!');
    
  } catch (error) {
    console.error(`Error in main function: ${error.message}`);
  }
}

// Run the main function
main().catch(console.error);