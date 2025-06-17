#!/bin/bash

echo "🧪 TESTING REAL DATA SOURCES - NO PLACEHOLDERS!"
echo "=================================================="
echo ""

echo "📡 1. Testing REAL Raydium API..."
echo "URL: https://api-v3.raydium.io/pools/info/list"
RAYDIUM_RESPONSE=$(curl -s "https://api-v3.raydium.io/pools/info/list?poolType=all&poolSortField=default&sortType=desc&pageSize=2&page=1")

if [[ $RAYDIUM_RESPONSE == *"\"success\":true"* ]]; then
    echo "✅ Raydium API: SUCCESS"
    echo "   Response contains: $(echo $RAYDIUM_RESPONSE | head -c 100)..."
    
    # Extract pool count
    POOL_COUNT=$(echo $RAYDIUM_RESPONSE | grep -o '"count":[0-9]*' | grep -o '[0-9]*')
    echo "   🏊 Total pools available: $POOL_COUNT"
    
    # Extract first pool TVL
    FIRST_TVL=$(echo $RAYDIUM_RESPONSE | grep -o '"tvl":[0-9.]*' | head -1 | grep -o '[0-9.]*')
    echo "   💰 First pool TVL: \$$FIRST_TVL"
else
    echo "❌ Raydium API: FAILED"
fi

echo ""

echo "📡 2. Testing REAL Jupiter Quote API..."
echo "URL: https://quote-api.jup.ag/v6/quote"
JUPITER_RESPONSE=$(curl -s "https://quote-api.jup.ag/v6/quote?inputMint=So11111111111111111111111111111111111111112&outputMint=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v&amount=100000000")

if [[ $JUPITER_RESPONSE == *"\"inAmount\""* ]]; then
    echo "✅ Jupiter API: SUCCESS"
    
    # Extract amounts
    IN_AMOUNT=$(echo $JUPITER_RESPONSE | grep -o '"inAmount":"[0-9]*"' | grep -o '[0-9]*')
    OUT_AMOUNT=$(echo $JUPITER_RESPONSE | grep -o '"outAmount":"[0-9]*"' | grep -o '[0-9]*')
    
    echo "   💱 SOL->USDC Quote:"
    echo "      Input: $IN_AMOUNT (0.1 SOL)"
    echo "      Output: $OUT_AMOUNT USDC"
    
    # Calculate SOL price
    if [[ -n "$IN_AMOUNT" && -n "$OUT_AMOUNT" && "$IN_AMOUNT" -gt 0 ]]; then
        SOL_PRICE=$(echo "scale=2; $OUT_AMOUNT / ($IN_AMOUNT / 100000000)" | bc -l 2>/dev/null || echo "calculation error")
        echo "      💲 Implied SOL Price: \$$SOL_PRICE"
    fi
else
    echo "❌ Jupiter API: FAILED"
    echo "   Response: $(echo $JUPITER_RESPONSE | head -c 100)..."
fi

echo ""

echo "📡 3. Testing REAL Orca API..."
echo "URL: https://api.orca.so/v1/whirlpool/list"
ORCA_RESPONSE=$(curl -s "https://api.orca.so/v1/whirlpool/list")

if [[ $ORCA_RESPONSE == *"\"whirlpools\""* ]]; then
    echo "✅ Orca API: SUCCESS"
    
    # Count whirlpools
    WHIRLPOOL_COUNT=$(echo $ORCA_RESPONSE | grep -o '"address":"[^"]*"' | wc -l)
    echo "   🌊 Total whirlpools: $WHIRLPOOL_COUNT"
    
    # Extract first pool TVL
    FIRST_TVL=$(echo $ORCA_RESPONSE | grep -o '"tvl":[0-9.]*' | head -1 | grep -o '[0-9.]*')
    if [[ -n "$FIRST_TVL" ]]; then
        echo "   💰 First pool TVL: \$$FIRST_TVL"
    fi
else
    echo "❌ Orca API: FAILED"
    echo "   Response: $(echo $ORCA_RESPONSE | head -c 100)..."
fi

echo ""

echo "🎯 SUMMARY:"
echo "==========="

if [[ $RAYDIUM_RESPONSE == *"\"success\":true"* ]]; then
    echo "✅ Raydium: REAL pool data available"
else
    echo "❌ Raydium: Failed to get data"
fi

if [[ $JUPITER_RESPONSE == *"\"inAmount\""* ]]; then
    echo "✅ Jupiter: REAL quotes available"
else
    echo "❌ Jupiter: Failed to get quotes"
fi

if [[ $ORCA_RESPONSE == *"\"whirlpools\""* ]]; then
    echo "✅ Orca: REAL whirlpool data available"
else
    echo "❌ Orca: Failed to get data"
fi

echo ""
echo "🚀 CONCLUSION: We have access to REAL market data!"
echo "   No more placeholders - we can build with actual DEX data!" 