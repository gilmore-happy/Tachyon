#!/bin/bash
# Run the MEV bot on devnet for testing with free SOL

echo "🚀 Starting MEV Bot on DEVNET..."
echo "📍 Using devnet RPC endpoints"

# Override mainnet URLs with devnet for testing
export RPC_URL=$DEVNET_RPC_URL
export RPC_URL_TX=$DEVNET_RPC_URL
export WSS_RPC_URL=$DEVNET_WSS_URL

# Load the rest of the environment
set -a
source .env
set +a

echo "💳 Wallet: $(solana-keygen pubkey $PAYER_KEYPAIR_PATH)"
echo "🌐 Network: DEVNET"
echo ""

# Run the bot
cargo run
