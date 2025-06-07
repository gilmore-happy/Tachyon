#!/bin/bash

# Script to generate a new configuration file for the Solana MEV Bot

# Make sure node.js is installed
if ! command -v node &> /dev/null; then
    echo "Error: Node.js is required but not installed. Please install Node.js first."
    exit 1
fi

# Run the configuration generator
node scripts/generate-config.js

# Make the configuration file more visible
echo ""
echo "Configuration complete! You can now run the bot with:"
echo "  cargo run --release"
echo ""
echo "Or edit the config.json file directly to make additional changes."