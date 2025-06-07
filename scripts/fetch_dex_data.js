// Script to fetch DEX market data using HyperBrowser MCP
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

// DEX API URLs
const DEX_APIS = {
  'raydium': 'https://api.raydium.io/v2/main/pairs',
  'raydium-clmm': 'https://api.raydium.io/v2/ammV3/ammPools',
  'orca-whirpools': 'https://api.mainnet.orca.so/v1/whirlpool/list',
  'meteora': 'https://dlmm-api.meteora.ag/pair/all'
};

// Function to fetch data using HyperBrowser MCP
async function fetchWithHyperBrowser(url) {
  try {
    console.log(`Fetching data from ${url}...`);
    
    // Generate a temporary file for storing the result
    const tempFile = path.join(__dirname, 'temp_response.json');
    
    // Command to use HyperBrowser MCP to fetch the data
    const command = `
    npx -y hyperbrowser-mcp <<EOF
    {
      "root": {
        "name": "crawl_webpages",
        "params": {
          "urls": ["${url}"]
        }
      }
    }
    EOF
    `;
    
    // Execute the command and capture output
    const result = execSync(command, { encoding: 'utf8' });
    
    // Parse the JSON result to extract the page content
    const mcp_result = JSON.parse(result);
    
    if (mcp_result.results && mcp_result.results.length > 0 && mcp_result.results[0].content) {
      // Try to parse the content as JSON
      try {
        return JSON.parse(mcp_result.results[0].content);
      } catch (e) {
        console.error(`Failed to parse content as JSON: ${e.message}`);
        return mcp_result.results[0].content;
      }
    } else {
      console.error('No content found in MCP result');
      return null;
    }
  } catch (error) {
    console.error(`Error fetching data from ${url}: ${error.message}`);
    return null;
  }
}

// Function to save data to cache file
function saveToCache(dexName, data) {
  const cacheDir = path.join(__dirname, '..', 'src', 'markets', 'cache');
  const cacheFile = path.join(cacheDir, `${dexName}-markets.json`);
  
  // Format the data based on the expected structure for each DEX
  let formattedData;
  
  switch (dexName) {
    case 'raydium-clmm':
      formattedData = { data: data || [] };
      break;
    case 'orca':
      formattedData = data || {};
      break;
    case 'orca-whirpools':
    case 'raydium':
    case 'meteora':
      formattedData = data || [];
      break;
    default:
      formattedData = data;
  }
  
  fs.writeFileSync(cacheFile, JSON.stringify(formattedData, null, 2));
  console.log(`Saved data to ${cacheFile}`);
}

// Main function to fetch and save all DEX data
async function fetchAllDexData() {
  for (const [dexName, apiUrl] of Object.entries(DEX_APIS)) {
    const data = await fetchWithHyperBrowser(apiUrl);
    if (data) {
      saveToCache(dexName, data);
    }
  }
  
  // For Orca, create an empty object if not already fetched
  const orcaCacheFile = path.join(__dirname, '..', 'src', 'markets', 'cache', 'orca-markets.json');
  if (!fs.existsSync(orcaCacheFile)) {
    saveToCache('orca', {});
  }
}

// Run the main function
fetchAllDexData().catch(error => {
  console.error(`Error in fetchAllDexData: ${error.message}`);
});