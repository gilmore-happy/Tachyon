# Data Storage Strategy for Solana Trading Bot

## 📊 Granular vs Aggregated Data Strategy

### Current Implementation (Granular)
We're storing **every arbitrage calculation** with full details:
- Every path evaluated (profitable or not)
- Complete route information
- All intermediate calculations
- Timestamp precision to milliseconds

### Benefits of Granular Data ✅
1. **Complete Audit Trail**: Track every decision the bot makes
2. **Performance Analysis**: Identify why certain paths succeed/fail
3. **Pattern Recognition**: Discover profitable patterns over time
4. **Debugging**: Trace back issues to specific calculations
5. **Machine Learning**: Train models on historical data

### Drawbacks of Granular Data ⚠️
1. **Storage Costs**: ~$0.25/GB/month (but minimal with 7-day TTL)
2. **Query Performance**: More data to scan (mitigated by GSIs)
3. **Write Costs**: $1.25 per million writes

## 💡 Recommended Hybrid Approach

### 1. **Real-time Granular Data** (Current Implementation)
- Store all calculations for 7 days
- Enables debugging and real-time analysis
- Auto-deleted by TTL to control costs

### 2. **Daily Aggregations** (To Add)
Create a new table `daily_summaries` with:
```json
{
  "date": "2025-06-04",
  "token_pair": "SOL-USDC",
  "total_opportunities": 1523,
  "profitable_paths": 89,
  "total_profit": 125.5,
  "best_profit": 5.2,
  "avg_profit": 1.41,
  "success_rate": 0.058,
  "top_dex_combo": ["Raydium", "Orca"],
  "peak_hour": 14
}
```

### 3. **Hourly Metrics** (Optional)
For high-frequency analysis:
```json
{
  "timestamp": "2025-06-04T14:00:00Z",
  "opportunities_per_minute": 25.4,
  "avg_gas_cost": 0.005,
  "network_congestion": 0.75
}
```

## 📈 Implementation Plan

### Phase 1: Granular Data (✅ Implemented)
- All calculations stored for 7 days
- TTL auto-cleanup
- GSIs for efficient queries

### Phase 2: Add Aggregation Lambda
```python
# AWS Lambda to run hourly
def aggregate_hourly_data():
    # Query last hour's data
    # Calculate summaries
    # Store in daily_summaries table
    # Optional: Send alerts for anomalies
```

### Phase 3: Analytics Dashboard
- Query aggregated data for trends
- Real-time metrics from granular data
- Historical performance charts

## 💰 Cost Optimization

### With 7-Day TTL:
- **Storage**: ~10GB max = $2.50/month
- **Writes**: 1M/day = $1.25/day = $37.50/month
- **Reads**: On-demand = ~$5-10/month
- **Total**: ~$50/month

### Without Granular Data:
- Would miss critical debugging capability
- Can't identify why profitable paths were missed
- No data for ML model training

## 🎯 Recommendation

**Keep the granular data with 7-day TTL** because:

1. **Debugging is Critical**: When a profitable opportunity is missed, you need to know why
2. **Pattern Discovery**: Granular data reveals patterns aggregates hide
3. **Cost is Reasonable**: ~$50/month for complete visibility
4. **ML Potential**: Historical data can train prediction models
5. **Compliance**: Full audit trail for any regulatory needs

### Future Optimizations:
1. **Selective Storage**: Only store paths with profit > 0.1%
2. **Compression**: Store route details in compressed format
3. **Tiered Storage**: Move old data to S3 for long-term analysis
4. **Stream Processing**: Use Kinesis for real-time aggregations

## 📊 Query Examples

### Find Today's Best Opportunities:
```javascript
// Using ProfitabilityIndex GSI
{
  IndexName: "ProfitabilityIndex",
  KeyConditionExpression: "#date = :today",
  ExpressionAttributeNames: {"#date": "date"},
  ExpressionAttributeValues: {":today": "2025-06-04"},
  ScanIndexForward: false, // Descending by profit
  Limit: 10
}
```

### Analyze SOL-USDC Performance:
```javascript
// Using TokenPairIndex GSI
{
  IndexName: "TokenPairIndex",
  KeyConditionExpression: "token_pair = :pair AND #ts > :yesterday",
  ExpressionAttributeNames: {"#ts": "timestamp"},
  ExpressionAttributeValues: {
    ":pair": "SOL-USDC",
    ":yesterday": Date.now() - 86400000
  }
}
```

## 🚀 Next Steps

1. **Monitor costs** for first week
2. **Implement aggregation** if queries slow down
3. **Add CloudWatch metrics** for performance monitoring
4. **Consider S3 export** for long-term analysis

The granular approach gives you the flexibility to optimize later while maintaining full visibility during the critical early stages of your bot's operation.
