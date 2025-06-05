# DynamoDB Setup Guide for Solana Trading Bot

## 🔄 Switching from MongoDB to DynamoDB

This guide helps you configure AWS DynamoDB as the database for your Solana Trading Bot.

## 📊 DynamoDB Table Structure

### 1. **swap_path_results** Table
- **Partition Key**: `path_id` (Number)
- **Sort Key**: `timestamp` (Number)
- **Attributes**:
  - `data` (String) - Full JSON of swap path result
  - `token_in` (String) - Input token address
  - `token_out` (String) - Output token address
  - `result` (Number) - Profit/loss calculation
  - `hops` (Number) - Number of swaps

### 2. **selected_paths** Table
- **Partition Key**: `selection_id` (String)
- **Sort Key**: `path_index` (Number)
- **Attributes**:
  - `data` (String) - Full JSON of selected path
  - `result` (Number) - Profitability score
  - `timestamp` (Number) - Selection timestamp

## 🚀 Setup Steps

### 1. AWS Configuration

Set up AWS credentials in your environment:

```bash
# Option 1: Environment Variables
export AWS_ACCESS_KEY_ID=your_access_key
export AWS_SECRET_ACCESS_KEY=your_secret_key
export AWS_REGION=us-east-1

# Option 2: AWS CLI Configuration
aws configure
```

### 2. Update Your Code

To use DynamoDB instead of MongoDB, update your imports:

```rust
// In files that use database functions, change:
use crate::common::database::{insert_swap_path_result_collection, insert_vec_swap_path_selected_collection};

// To:
use crate::common::database_dynamodb::{insert_swap_path_result_collection, insert_vec_swap_path_selected_collection};
```

### 3. Add to mod.rs

Update `src/common/mod.rs`:

```rust
pub mod config;
pub mod constants;
pub mod database;
pub mod database_dynamodb;  // Add this line
pub mod debug;
pub mod maths;
pub mod types;
pub mod utils;
```

### 4. Environment Variables for Codex

Add these to your Codex environment:

```bash
# AWS Configuration
AWS_REGION=us-east-1
AWS_ACCESS_KEY_ID=<your-access-key>
AWS_SECRET_ACCESS_KEY=<your-secret-key>

# Optional: Use IAM role instead (for EC2/ECS)
# AWS_USE_IAM_ROLE=true
```

### 5. Create Tables (One-time Setup)

Run this initialization code once:

```rust
use crate::common::database_dynamodb::DynamoDBClient;

#[tokio::main]
async fn main() -> Result<()> {
    let client = DynamoDBClient::new().await?;
    client.create_tables_if_not_exist().await?;
    println!("✅ DynamoDB tables created successfully!");
    Ok(())
}
```

## 💰 Cost Comparison

### DynamoDB (Pay-per-request mode)
- **Write**: $1.25 per million requests
- **Read**: $0.25 per million requests
- **Storage**: $0.25 per GB/month
- **No upfront costs or minimum fees**

### MongoDB Atlas (Managed)
- **M0 (Free)**: Limited to 512MB
- **M10 (Dedicated)**: ~$57/month
- **Storage**: Included in tier

### Cost Example (1M trades/day)
- **DynamoDB**: ~$40/month
- **MongoDB M10**: $57/month (fixed)

## 🔧 Advanced Configuration

### Global Secondary Indexes (GSI)

For better query performance, add GSIs:

```javascript
// GSI for querying by token pair
{
  IndexName: "TokenPairIndex",
  PartitionKey: "token_in",
  SortKey: "token_out",
  Projection: "ALL"
}

// GSI for querying by profitability
{
  IndexName: "ProfitabilityIndex",
  PartitionKey: "token_in",
  SortKey: "result",
  Projection: "ALL"
}
```

### Time-to-Live (TTL)

Enable automatic data cleanup:

```javascript
// Enable TTL on timestamp attribute
// Data older than 30 days will be automatically deleted
{
  AttributeName: "ttl",
  Enabled: true
}
```

## 🛡️ Security Best Practices

1. **Use IAM Roles** when running on AWS infrastructure
2. **Limit Permissions** - Only grant read/write to specific tables
3. **Enable Encryption** - Use AWS KMS for encryption at rest
4. **Monitor Usage** - Set up CloudWatch alarms for anomalies

## 📈 Monitoring

Set up CloudWatch dashboards to monitor:
- Read/Write capacity consumed
- Throttled requests
- System errors
- Latency metrics

## 🔄 Migration from MongoDB

If you have existing MongoDB data:

1. Export MongoDB data to JSON
2. Transform to DynamoDB format
3. Use AWS Data Pipeline or custom script to import

## ✅ Benefits of DynamoDB for Trading Bots

1. **Serverless**: No infrastructure to manage
2. **Auto-scaling**: Handles traffic spikes automatically
3. **Low Latency**: Single-digit millisecond performance
4. **High Availability**: 99.999% SLA
5. **Cost-effective**: Pay only for what you use
6. **AWS Integration**: Works seamlessly with other AWS services

## 🚨 Important Notes

- DynamoDB has a 400KB item size limit
- Design partition keys to avoid hot partitions
- Use batch operations for better performance
- Consider DynamoDB Streams for real-time processing
