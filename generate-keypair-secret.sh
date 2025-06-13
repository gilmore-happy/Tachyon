#!/bin/bash

# Helper script to generate base64-encoded keypair for Codex secrets

echo "🔐 Keypair Base64 Encoder for Codex Setup"
echo "========================================"
echo ""

# Check if keypair path is provided as argument
if [ $# -eq 0 ]; then
    # Try to read from .env file
    if [ -f ".env" ]; then
        KEYPAIR_PATH=$(grep "PAYER_KEYPAIR_PATH=" .env | cut -d '=' -f2)
        if [ -z "$KEYPAIR_PATH" ]; then
            echo "❌ Error: No keypair path found in .env file"
            echo ""
            echo "Usage: $0 <path-to-keypair.json>"
            echo "Example: $0 /home/galt/solana-wallet/mev-bot-keypair.json"
            exit 1
        fi
        echo "📁 Using keypair path from .env: $KEYPAIR_PATH"
    else
        echo "❌ Error: No keypair path provided"
        echo ""
        echo "Usage: $0 <path-to-keypair.json>"
        echo "Example: $0 /home/galt/solana-wallet/mev-bot-keypair.json"
        exit 1
    fi
else
    KEYPAIR_PATH=$1
fi

# Check if file exists
if [ ! -f "$KEYPAIR_PATH" ]; then
    echo "❌ Error: Keypair file not found at: $KEYPAIR_PATH"
    exit 1
fi

# Generate base64 encoded content
echo ""
echo "🔄 Encoding keypair file..."
ENCODED=$(base64 -w 0 "$KEYPAIR_PATH")

if [ $? -eq 0 ]; then
    echo "✅ Success! Your base64-encoded keypair is ready."
    echo ""
    echo "📋 Copy the following value for your PAYER_KEYPAIR_CONTENT secret:"
    echo ""
    echo "========== START COPY HERE =========="
    echo "$ENCODED"
    echo "=========== END COPY HERE ==========="
    echo ""
    echo "⚠️  IMPORTANT SECURITY NOTES:"
    echo "   - This is your private key - keep it secure!"
    echo "   - Only paste this into the Codex secrets configuration"
    echo "   - Never share this value or commit it to git"
    echo "   - Consider using a dedicated wallet with limited funds"
    echo ""
    
    # Optionally save to a temporary file
    read -p "💾 Save to a temporary file? (y/n): " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        TEMP_FILE="keypair-secret-$(date +%s).txt"
        echo "$ENCODED" > "$TEMP_FILE"
        chmod 600 "$TEMP_FILE"
        echo "✅ Saved to: $TEMP_FILE"
        echo "   Remember to delete this file after use!"
    fi
else
    echo "❌ Error: Failed to encode keypair file"
    exit 1
fi
