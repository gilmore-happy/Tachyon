#!/bin/bash

echo "Starting error and warning fixes for Tachyon project..."

# 1. Fix import references from_Pubkey to from_pubkey
echo "Fixing import references..."
find /home/galt/Tachyon/src -type f -name "*.rs" -exec sed -i 's/from_Pubkey, from_str/from_pubkey, from_str/g' {} \;
find /home/galt/Tachyon/src -type f -name "*.rs" -exec sed -i 's/from_Pubkey, /from_pubkey, /g' {} \;
find /home/galt/Tachyon/src -type f -name "*.rs" -exec sed -i 's/from_Pubkey}/from_pubkey}/g' {} \;

# 2. Fix function calls from_Pubkey to from_pubkey
echo "Fixing function calls..."
find /home/galt/Tachyon/src -type f -name "*.rs" -exec sed -i 's/from_Pubkey(/from_pubkey(/g' {} \;

# 3. Fix unused variables
echo "Fixing unused variables..."
sed -i 's/account_subscription_client/\_account_subscription_client/g' /home/galt/Tachyon/src/markets/orca_whirpools.rs
sed -i 's/account_subscription_client/\_account_subscription_client/g' /home/galt/Tachyon/src/markets/raydium.rs
sed -i 's/let result =/let _result =/g' /home/galt/Tachyon/src/markets/raydium.rs
sed -i 's/let bytes_slice =/let _bytes_slice =/g' /home/galt/Tachyon/src/markets/raydium.rs

# 4. Fix unused associated tokens in create_transaction.rs
sed -i 's/let associated_token_in =/let _associated_token_in =/g' /home/galt/Tachyon/src/transactions/create_transaction.rs
sed -i 's/let associated_token_out =/let _associated_token_out =/g' /home/galt/Tachyon/src/transactions/create_transaction.rs

# 5. Fix params assignments and references
echo "Fixing params assignments and references..."
# For orca_whirpools.rs - create and update the variable properly
sed -i 's/let mut params: String = String::new();/let mut params = String::new();/g' /home/galt/Tachyon/src/markets/orca_whirpools.rs
# For raydium.rs - create and update the variable properly  
sed -i 's/let mut params: String = String::new();/let mut params = String::new();/g' /home/galt/Tachyon/src/markets/raydium.rs

# 6. Fix cfg_attr issue and remove duplicate Debug derive
echo "Fixing cfg_attr feature issue and remove duplicate Debug derive..."
sed -i 's/#\[cfg_attr(feature = "client", derive(Debug))\]/\/\/ Removed cfg_attr derive/g' /home/galt/Tachyon/src/markets/raydium.rs

# 7. Remove unused imports
echo "Removing unused imports..."
sed -i 's/use solana_client::nonblocking::rpc_client::RpcClient;//g' /home/galt/Tachyon/src/markets/raydium.rs

echo "Fixes applied. Running cargo check to verify..."
cd /home/galt/Tachyon && cargo check

echo "Fix script completed!"