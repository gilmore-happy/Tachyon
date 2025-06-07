#!/bin/bash

# Script to fetch DEX data for cache files
echo "Fetching DEX data for Solana Trading Bot cache..."

# Make sure node.js is installed
if ! command -v node &> /dev/null; then
    echo "Error: Node.js is required but not installed. Please install Node.js first."
    exit 1
fi

# Run the fetch script
node scripts/fetch_dex_data.js

echo "Data fetching complete. Check the cache files to verify data was populated."
echo "Cache files are located in: src/markets/cache/"

# List the cache files and their sizes
echo "Cache file sizes:"
ls -lh src/markets/cache/