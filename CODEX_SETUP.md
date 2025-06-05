# Codex Setup Guide for Solana Trading Bot

This guide will help you configure OpenAI Codex to work with your Solana Trading Bot project.

## 📋 Prerequisites

1. Access to [chatgpt.com/codex](https://chatgpt.com/codex)
2. GitHub repository connected to Codex
3. Your Solana wallet keypair file
4. RPC endpoint URLs (QuickNode, Helius, etc.)

## 🔐 Preparing Secrets

### 1. Base64 Encode Your Keypair

First, you need to convert your keypair file to base64 format:

```bash
# If your keypair is at /home/galt/solana-wallet/mev-bot-keypair.json
base64 -w 0 /home/galt/solana-wallet/mev-bot-keypair.json
```

Copy the output - this will be your `PAYER_KEYPAIR_CONTENT` secret.

### 2. Extract API Keys from URLs

From your RPC URLs, extract the API keys:
- QuickNode: The part after the last `/` in the URL
- Helius: The `api-key` parameter value

## ⚙️ Codex Configuration

### Environment Variables (Non-sensitive)

In Codex, set these environment variables:

```bash
# Network Configuration
NETWORK_MODE=mainnet              # or "devnet" for testing
DATABASE_NAME=mev_bot_solana
MAX_RETRIES=5
SEND_RETRY_COUNT=3
PRIORITY_FEE_LUT=100
MAX_RETRIES_LUT=5
LUT_BUFFER_COUNT=10

# Build Configuration
RUST_LOG=info
CARGO_TERM_COLOR=always

# Trading Configuration
ENABLE_PAPER_TRADING=false        # Set to "true" for paper trading
```

### Secrets (Sensitive data)

In Codex secrets section, add:

```bash
# RPC Endpoints (replace with your actual endpoints)
DEVNET_RPC_URL=https://radial-icy-meme.solana-devnet.quiknode.pro/YOUR_KEY/
DEVNET_WSS_URL=wss://radial-icy-meme.solana-devnet.quiknode.pro/YOUR_KEY/
MAINNET_RPC_URL=https://sly-convincing-leaf.solana-mainnet.quiknode.pro/YOUR_KEY/
WSS_RPC_URL=wss://sly-convincing-leaf.solana-mainnet.quiknode.pro/YOUR_KEY/
BLOCK_ENGINE_URL=https://mainnet.helius-rpc.com/?api-key=YOUR_KEY

# Wallet Configuration
PAYER_KEYPAIR_PATH=/workspace/wallet/keypair.json
PAYER_KEYPAIR_CONTENT=<your-base64-encoded-keypair>

# API Keys (optional, if you want to store them separately)
QUICKNODE_API_KEY=<your-quicknode-key>
HELIUS_API_KEY=<your-helius-key>

# MongoDB (optional, if using)
MONGODB_URI=mongodb://username:password@host:port/database
```

### Setup Script

The `setup.sh` script is already configured in this repository and will:
1. Install Rust and Solana CLI
2. Set up your wallet from the base64 secret
3. Build the project
4. Create necessary directories
5. Generate default configuration files

## 🚀 Running Tasks in Codex

### Example Tasks for Ask Mode

1. **Code Review**
   ```
   Review the arbitrage calculation logic in src/arbitrage/calc_arb.rs 
   and suggest optimizations for reducing latency.
   ```

2. **Architecture Analysis**
   ```
   Analyze the current MEV strategy implementation and suggest improvements 
   for capturing more profitable opportunities.
   ```

3. **Security Audit**
   ```
   Check for potential security vulnerabilities in the transaction handling 
   and wallet management code.
   ```

### Example Tasks for Code Mode

1. **Add New DEX Integration**
   ```
   Add support for Phoenix DEX to the markets module, following the 
   existing pattern used for Raydium and Orca.
   ```

2. **Optimize Gas Usage**
   ```
   Implement dynamic priority fee adjustment based on network congestion 
   in src/fees/priority_fees.rs.
   ```

3. **Improve Error Handling**
   ```
   Add comprehensive error handling and retry logic to the transaction 
   submission process in src/execution/executor.rs.
   ```

## 🔍 Debugging Codex Issues

If Codex encounters issues:

1. **Build Failures**
   - Check that all dependencies are properly specified in Cargo.toml
   - Ensure the Rust version is compatible

2. **Network Issues**
   - Verify the proxy settings are correctly configured
   - Check that RPC endpoints are accessible

3. **Wallet Issues**
   - Ensure the base64 encoding is correct
   - Verify the keypair file permissions (should be 600)

## 📊 Monitoring Codex Tasks

When Codex is working on your task:
- It will clone your repository
- Run the setup script
- Execute the requested changes
- Run tests to verify the changes
- Present a diff for review

## ⚠️ Security Best Practices

1. **Use a Dedicated Wallet**: Create a separate wallet for bot operations with limited funds
2. **Rotate API Keys**: Regularly update your RPC endpoint API keys
3. **Review All Changes**: Carefully review all diffs before accepting PRs
4. **Test on Devnet**: Always test changes on devnet before mainnet deployment
5. **Limit Permissions**: Only grant Codex access to necessary repositories

## 🆘 Troubleshooting

### Common Issues

1. **"Cannot connect to RPC"**
   - Verify your RPC URLs are correct in secrets
   - Check if the API keys are valid

2. **"Wallet not found"**
   - Ensure PAYER_KEYPAIR_CONTENT is properly base64 encoded
   - Check the setup script output for wallet configuration errors

3. **"Build failed"**
   - Review the Rust version compatibility
   - Check for missing system dependencies

### Getting Help

If you encounter issues:
1. Check the setup script output for specific error messages
2. Review the AGENTS.md file for project-specific guidelines
3. Ensure all secrets are properly configured
4. Verify network connectivity through the proxy

## 📝 Next Steps

1. Configure your Codex environment with the variables and secrets above
2. Run a test task in ask mode to verify the setup
3. Try a simple code modification task
4. Gradually increase task complexity as you become familiar with Codex

Remember to always review and test changes thoroughly before deploying to production!
