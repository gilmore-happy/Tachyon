# Solana Trading Bot - Agent Guidelines

## Overview
This is a high-performance Solana MEV/arbitrage bot written in Rust. The bot identifies and executes profitable trading opportunities across multiple DEXs on Solana.

## Project Structure
- `src/` - Main bot source code
  - `arbitrage/` - Arbitrage detection and calculation logic
  - `execution/` - Trade execution and paper trading
  - `markets/` - DEX integrations (Raydium, Orca, Meteora)
  - `transactions/` - Transaction building and submission
  - `common/` - Shared utilities and configuration
  - `fees/` - Fee calculation and priority fee strategies
  - `data/` - Data structures and graphs
- `programs/` - On-chain programs (lb_clmm, raydium_amm)
- `logs/` - Application logs (errors.log, program.log)

## Development Guidelines

### Running the Bot
- **Devnet Testing**: `./run-devnet.sh` - Uses free devnet SOL for testing
- **Paper Trading**: `./run-paper-trading.sh` - Simulates trades on mainnet without real execution
- **Production**: `cargo run --release` - Live trading (use with caution)

### Building and Testing
```bash
# Build the project
cargo build --release

# Run all tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo test

# Run specific test
cargo test test_name

# Check code quality
cargo fmt -- --check
cargo clippy -- -D warnings
```

### Code Standards
- Always run `cargo fmt` before committing code
- Fix all `cargo clippy` warnings
- Ensure all tests pass before submitting changes
- Add tests for new functionality
- Document complex algorithms and strategies

### Key Commands
- Build: `cargo build --release`
- Format: `cargo fmt`
- Lint: `cargo clippy`
- Test: `cargo test`
- Run: `cargo run --release`
- Watch: `cargo watch -x run` (requires cargo-watch)

### Environment Configuration
The bot uses environment variables for configuration. Key variables:
- `NETWORK_MODE` - Set to "devnet" or "mainnet"
- `ENABLE_PAPER_TRADING` - Set to "true" for paper trading
- `DATABASE_NAME` - MongoDB database name
- `MAX_RETRIES` - Maximum retry attempts for transactions
- `PRIORITY_FEE_LUT` - Priority fee for lookup table operations

### Important Files
- `.env` - Environment configuration (DO NOT COMMIT - contains secrets)
- `config.json` - Bot trading configuration (tokens, thresholds)
- `logging_config.yaml` - Logging configuration
- `Cargo.toml` - Rust dependencies and project configuration

### Trading Configuration
The bot can be configured via `config.json`:
```json
{
  "tokens_to_trade": [
    {
      "address": "token_mint_address",
      "symbol": "TOKEN"
    }
  ],
  "min_profit_threshold": 20.0,
  "max_slippage": 0.02,
  "enable_paper_trading": false
}
```

### Security Best Practices
- Never commit private keys, API keys, or RPC URLs
- Use environment variables for all sensitive data
- Test thoroughly on devnet before mainnet deployment
- Use a dedicated wallet with limited funds for the bot
- Monitor bot activity regularly
- Implement proper error handling and logging

### Debugging Tips
- Check `logs/errors.log` for error messages
- Check `logs/program.log` for general activity
- Use `RUST_LOG=debug` for verbose logging
- Monitor Solana explorer for transaction status
- Use paper trading mode to test strategies safely

### Performance Optimization
- The bot is optimized for low latency with:
  - Release build optimizations (LTO, single codegen unit)
  - Efficient data structures (DashMap for concurrent access)
  - Async/await for non-blocking operations
  - Connection pooling for RPC requests

### Common Issues and Solutions
1. **RPC Rate Limits**: Use multiple RPC endpoints or upgrade your plan
2. **Transaction Failures**: Check priority fees and retry logic
3. **Memory Usage**: Monitor with `systemstat` integration
4. **Network Congestion**: Implement dynamic fee adjustment

## Agent-Specific Instructions

### When Making Changes
1. Always preserve existing functionality unless explicitly asked to modify
2. Run tests after making changes to ensure nothing breaks
3. Update documentation if changing APIs or adding features
4. Consider performance implications of changes

### Code Review Focus Areas
- Security vulnerabilities in transaction handling
- Proper error handling and recovery
- Efficient use of RPC calls
- Correct decimal/precision handling for tokens
- Race condition prevention in concurrent operations

### Testing Requirements
- Unit tests for calculation logic
- Integration tests for DEX interactions
- Simulation tests for arbitrage strategies
- Paper trading validation before production

### Deployment Checklist
- [ ] All tests passing
- [ ] Code formatted and linted
- [ ] Configuration validated
- [ ] Wallet has sufficient SOL for fees
- [ ] RPC endpoints are responsive
- [ ] Database connection verified
- [ ] Logging configured properly
