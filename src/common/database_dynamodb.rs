use anyhow::Result;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, AttributeValue, BillingMode, KeySchemaElement, KeyType,
    ScalarAttributeType,
};
use aws_sdk_dynamodb::Client;
use log::info;
use serde_json;
use std::collections::HashMap;

use crate::arbitrage::types::{SwapPathResult, VecSwapPathSelected};

pub struct DynamoDBClient {
    client: Client,
}

impl DynamoDBClient {
    pub async fn new() -> Result<Self> {
        let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
        let config = aws_config::from_env().region(region_provider).load().await;
        let client = Client::new(&config);

        Ok(Self { client })
    }

    pub async fn insert_swap_path_result(
        &self,
        table_name: &str,
        sp_result: SwapPathResult,
    ) -> Result<()> {
        // Convert the struct to a JSON string then to DynamoDB AttributeValues
        let json_string_val = serde_json::to_string(&sp_result)?;
        let json_value: serde_json::Value = serde_json::from_str(&json_string_val)?;

        let mut item = HashMap::new();

        // Primary key - using path_id and timestamp
        item.insert(
            "path_id".to_string(),
            AttributeValue::N(sp_result.path_id.to_string()),
        );

        // Add timestamp as sort key for time-series data
        let now = chrono::Utc::now();
        item.insert(
            "timestamp".to_string(),
            AttributeValue::N(now.timestamp_millis().to_string()),
        );

        // Add TTL for automatic cleanup after 7 days
        let ttl_timestamp = now.timestamp() + (7 * 24 * 60 * 60); // 7 days in seconds
        item.insert(
            "ttl".to_string(),
            AttributeValue::N(ttl_timestamp.to_string()),
        );

        // Add date partition for better query performance
        item.insert(
            "date".to_string(),
            AttributeValue::S(now.format("%Y-%m-%d").to_string()),
        );

        // Add token pair for GSI queries
        item.insert(
            "token_pair".to_string(),
            AttributeValue::S(format!(
                "{}-{}",
                sp_result.token_in_symbol, sp_result.token_out_symbol
            )),
        );

        // Store the full data as JSON
        item.insert("data".to_string(), AttributeValue::S(json_string_val));

        // Add searchable attributes
        item.insert(
            "token_in".to_string(),
            AttributeValue::S(sp_result.token_in.clone()),
        );
        item.insert(
            "token_out".to_string(),
            AttributeValue::S(sp_result.token_out.clone()),
        );
        item.insert(
            "result".to_string(),
            AttributeValue::N(sp_result.result.to_string()),
        );
        item.insert(
            "hops".to_string(),
            AttributeValue::N(sp_result.hops.to_string()),
        );

        self.client
            .put_item()
            .table_name(table_name)
            .set_item(Some(item))
            .send()
            .await?;

        info!(
            "📊 SwapPathResult written to DynamoDB table: {}",
            table_name
        );
        Ok(())
    }

    pub async fn insert_vec_swap_path_selected(
        &self,
        table_name: &str,
        best_paths: VecSwapPathSelected,
    ) -> Result<()> {
        // Insert each path separately for better queryability
        for (index, path) in best_paths.value.iter().enumerate() {
            let json_string = serde_json::to_string(&path)?;

            let mut item = HashMap::new();

            // Composite primary key
            item.insert(
                "selection_id".to_string(),
                AttributeValue::S(format!(
                    "selection_{}",
                    chrono::Utc::now().timestamp_millis()
                )),
            );
            item.insert(
                "path_index".to_string(),
                AttributeValue::N(index.to_string()),
            );

            // Store the full data
            item.insert("data".to_string(), AttributeValue::S(json_string));

            // Add searchable attributes
            item.insert(
                "result".to_string(),
                AttributeValue::N(path.result.to_string()),
            );
            item.insert(
                "timestamp".to_string(),
                AttributeValue::N(chrono::Utc::now().timestamp_millis().to_string()),
            );

            self.client
                .put_item()
                .table_name(table_name)
                .set_item(Some(item))
                .send()
                .await?;
        }

        info!(
            "📊 VecSwapPathSelected written to DynamoDB table: {}",
            table_name
        );
        Ok(())
    }

    pub async fn create_tables_if_not_exist(&self) -> Result<()> {
        // Create swap_path_results table
        match self
            .client
            .create_table()
            .table_name("swap_path_results")
            .key_schema(
                KeySchemaElement::builder()
                    .attribute_name("path_id")
                    .key_type(KeyType::Hash)
                    .build()?,
            )
            .key_schema(
                KeySchemaElement::builder()
                    .attribute_name("timestamp")
                    .key_type(KeyType::Range)
                    .build()?,
            )
            .attribute_definitions(
                AttributeDefinition::builder()
                    .attribute_name("path_id")
                    .attribute_type(ScalarAttributeType::N)
                    .build()?,
            )
            .attribute_definitions(
                AttributeDefinition::builder()
                    .attribute_name("timestamp")
                    .attribute_type(ScalarAttributeType::N)
                    .build()?,
            )
            .billing_mode(BillingMode::PayPerRequest)
            .send()
            .await
        {
            Ok(_) => info!("✅ Created swap_path_results table"),
            Err(e) => {
                if !e.to_string().contains("ResourceInUseException") {
                    return Err(e.into());
                }
            }
        }

        // Create selected_paths table
        match self
            .client
            .create_table()
            .table_name("selected_paths")
            .key_schema(
                KeySchemaElement::builder()
                    .attribute_name("selection_id")
                    .key_type(KeyType::Hash)
                    .build()?,
            )
            .key_schema(
                KeySchemaElement::builder()
                    .attribute_name("path_index")
                    .key_type(KeyType::Range)
                    .build()?,
            )
            .attribute_definitions(
                AttributeDefinition::builder()
                    .attribute_name("selection_id")
                    .attribute_type(ScalarAttributeType::S)
                    .build()?,
            )
            .attribute_definitions(
                AttributeDefinition::builder()
                    .attribute_name("path_index")
                    .attribute_type(ScalarAttributeType::N)
                    .build()?,
            )
            .billing_mode(BillingMode::PayPerRequest)
            .send()
            .await
        {
            Ok(_) => info!("✅ Created selected_paths table"),
            Err(e) => {
                if !e.to_string().contains("ResourceInUseException") {
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }
}

// Alternative implementation using the original function signatures
pub async fn insert_swap_path_result_collection(
    table_name: &str,
    sp_result: SwapPathResult,
) -> Result<()> {
    let client = DynamoDBClient::new().await?;
    client.insert_swap_path_result(table_name, sp_result).await
}

pub async fn insert_vec_swap_path_selected_collection(
    table_name: &str,
    best_paths_for_strat: VecSwapPathSelected,
) -> Result<()> {
    let client = DynamoDBClient::new().await?;
    client
        .insert_vec_swap_path_selected(table_name, best_paths_for_strat)
        .await
}
