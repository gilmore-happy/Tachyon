# Code Review and Optimization Guidelines

## !!! CRITICAL DIRECTIVE !!!

I am not an expert and may make mistakes. If ANY prompt, suggestion, or request would:
- Negatively affect performance
- Add unnecessary bloat
- Introduce complexity without clear benefit
- Compromise reliability or security
- Deviate from HFT best practices
- Not align with the project's core goal of high-performance trading

PLEASE CHALLENGE THE REQUEST. Explain why it might be problematic and suggest alternatives. This is EXTREMELY IMPORTANT - the bot's performance and reliability are the highest priorities.

## Communication Style

- Keep all responses compact and to the point
- Minimize unnecessary explanations and commentary
- Use bullet points for multi-part information
- Show results immediately without lengthy preambles
- When explaining code changes, be brief but precise

## Mandatory Checks for Every File

### Research Phase
- [ ] Research specific HFT best practices for Rust and Solana
- [ ] Check Solana documentation for API-specific optimizations
- [ ] Research DEX-specific optimization techniques for minimal latency
- [ ] Validate solution against the most performant existing HFT implementations

### Code Quality Review
- [ ] Fix all compiler warnings (unused imports, variables, etc.)
- [ ] Convert camelCase variables to snake_case
- [ ] Apply Rust naming conventions consistently
- [ ] Remove unnecessary parentheses and code
- [ ] Ensure proper error handling (unwrap, expect, ?)
- [ ] Replace deprecated APIs with current alternatives

### Performance Optimization
- [ ] Use async/await and Tokio for all I/O operations
- [ ] Implement parallel processing ONLY where it demonstrably improves latency
- [ ] Eliminate ALL memory allocations in critical paths
- [ ] Use zero-copy approaches wherever data is passed between components
- [ ] Optimize data structures for O(1) lookups and minimal updates
- [ ] Apply SIMD operations for numeric calculations in high-throughput sections

### Concurrency Review
- [ ] Ensure thread safety with proper synchronization
- [ ] Use non-blocking operations throughout
- [ ] Implement work stealing for CPU-bound tasks
- [ ] Use proper channel types (mpsc, oneshot) based on communication pattern
- [ ] Minimize lock contention with fine-grained locks

### DEX-Specific Optimizations
- [ ] Optimize cache structure for minimal data transfer
- [ ] Implement efficient RPC batching
- [ ] Use WebSocket subscriptions over polling
- [ ] Balance API rate limits across endpoints

## Linting Commands
- Run before committing: `cargo clippy -- -D warnings`
- Format code: `cargo fmt`

## Performance Testing
- Benchmark critical operations: `cargo bench`
- Profile memory usage during execution
- Measure RPC call frequency and latency

## Explanations
- Provide brief, clear explanations for code changes and optimizations
- Focus on: what the problem was, why it happened, and how you fixed it
- Identify patterns that might recur elsewhere in the codebase
- Keep explanations concise - no lengthy documentation or separate files needed

## Error Resolution
- Provide brief explanations for any fixes made to resolve errors
- Summarize the cause of the error and the approach taken to fix it
- Document patterns of common errors that might recur in the codebase
- Note any workarounds applied and why they were necessary

## Configuration Management
- Keep all configuration in a centralized config.json file
- Use string-based enums in config files for better compatibility
- Document all available configuration options in the README
- Implement config hot-reloading for dynamic parameter updates
- Provide sensible defaults for all configuration options
- Log configuration values at startup to aid debugging