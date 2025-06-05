#!/bin/bash

# Script to create DynamoDB tables for Solana Trading Bot
# This creates optimized tables with Global Secondary Indexes

echo "🚀 Creating DynamoDB Tables for Solana Trading Bot"
echo "=================================================="

# Set AWS region
AWS_REGION=${AWS_REGION:-us-east-1}
echo "📍 Using AWS Region: $AWS_REGION"

# Function to check if table exists
table_exists() {
    aws dynamodb describe-table --table-name $1 --region $AWS_REGION >/dev/null 2>&1
    return $?
}

# Create swap_path_results table
echo ""
echo "📊 Creating swap_path_results table..."

if table_exists "swap_path_results"; then
    echo "⚠️  Table swap_path_results already exists"
else
    aws dynamodb create-table \
        --table-name swap_path_results \
        --attribute-definitions \
            AttributeName=path_id,AttributeType=N \
            AttributeName=timestamp,AttributeType=N \
            AttributeName=token_pair,AttributeType=S \
            AttributeName=date,AttributeType=S \
            AttributeName=result,AttributeType=N \
        --key-schema \
            AttributeName=path_id,KeyType=HASH \
            AttributeName=timestamp,KeyType=RANGE \
        --global-secondary-indexes \
            '[
                {
                    "IndexName": "TokenPairIndex",
                    "KeySchema": [
                        {"AttributeName": "token_pair", "KeyType": "HASH"},
                        {"AttributeName": "timestamp", "KeyType": "RANGE"}
                    ],
                    "Projection": {"ProjectionType": "ALL"}
                },
                {
                    "IndexName": "ProfitabilityIndex",
                    "KeySchema": [
                        {"AttributeName": "date", "KeyType": "HASH"},
                        {"AttributeName": "result", "KeyType": "RANGE"}
                    ],
                    "Projection": {"ProjectionType": "ALL"}
                }
            ]' \
        --billing-mode PAY_PER_REQUEST \
        --region $AWS_REGION

    if [ $? -eq 0 ]; then
        echo "✅ Table swap_path_results created successfully!"
    else
        echo "❌ Failed to create swap_path_results table"
        exit 1
    fi
fi

# Create selected_paths table
echo ""
echo "📊 Creating selected_paths table..."

if table_exists "selected_paths"; then
    echo "⚠️  Table selected_paths already exists"
else
    aws dynamodb create-table \
        --table-name selected_paths \
        --attribute-definitions \
            AttributeName=selection_id,AttributeType=S \
            AttributeName=path_index,AttributeType=N \
        --key-schema \
            AttributeName=selection_id,KeyType=HASH \
            AttributeName=path_index,KeyType=RANGE \
        --billing-mode PAY_PER_REQUEST \
        --region $AWS_REGION

    if [ $? -eq 0 ]; then
        echo "✅ Table selected_paths created successfully!"
    else
        echo "❌ Failed to create selected_paths table"
        exit 1
    fi
fi

# Enable TTL on swap_path_results
echo ""
echo "⏰ Enabling TTL on swap_path_results table..."

aws dynamodb update-time-to-live \
    --table-name swap_path_results \
    --time-to-live-specification "Enabled=true,AttributeName=ttl" \
    --region $AWS_REGION >/dev/null 2>&1

if [ $? -eq 0 ]; then
    echo "✅ TTL enabled on swap_path_results"
else
    echo "⚠️  Could not enable TTL (this is optional)"
fi

# List tables
echo ""
echo "📋 Your DynamoDB tables:"
aws dynamodb list-tables --region $AWS_REGION | jq -r '.TableNames[]' | grep -E "(swap_path_results|selected_paths)" | while read table; do
    echo "   ✓ $table"
done

echo ""
echo "🎉 DynamoDB setup complete!"
echo ""
echo "📝 Next steps:"
echo "   1. Run: cargo run --bin test_dynamodb"
echo "   2. Check AWS Console to verify tables"
echo "   3. Update your code to use database_dynamodb module"
echo ""
echo "⚠️  Security reminder: Rotate your AWS credentials regularly!"
