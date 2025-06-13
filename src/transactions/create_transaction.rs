use anchor_spl::token::spl_token;
use anyhow::Result;
// Removed unused import: itertools::Itertools
use log::error;
use log::info;
use log::warn;
use serde::{Deserialize, Serialize};
use solana_client::{
    connection_cache::ConnectionCache,
    nonblocking::rpc_client::RpcClient as NonBlockingRpcClient, // Alias for clarity
    rpc_client::RpcClient, // Keep sync for parts that might remain sync or for type matching if complex
    rpc_config::{RpcSendTransactionConfig, RpcSimulateTransactionConfig},
    tpu_client::TpuClientConfig,
};
use solana_sdk::{
    address_lookup_table::{
        instruction::{create_lookup_table, extend_lookup_table},
        state::AddressLookupTable,
        AddressLookupTableAccount,
    },
    commitment_config::{CommitmentConfig, CommitmentLevel},
    compute_budget::ComputeBudgetInstruction,
    instruction::Instruction,
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
    transaction::VersionedTransaction,
};
use solana_transaction_status::UiTransactionEncoding;
use spl_associated_token_account::get_associated_token_address;
use spl_associated_token_account::instruction::create_associated_token_account;
use std::io::{BufWriter, Write};
use std::{
    fs::OpenOptions,
    io::BufReader,
    path::Path,
    sync::Arc,
}; // Added warn

use super::{
    meteoradlmm_swap::{construct_meteora_instructions, SwapParametersMeteora},
    orca_whirpools_swap::{construct_orca_whirpools_instructions, SwapParametersOrcaWhirpools},
    raydium_swap::{construct_raydium_instructions, SwapParametersRaydium},
};
use crate::{
    arbitrage::types::SwapPathResult,
    common::{constants::Env, utils::from_str},
    markets::types::DexLabel,
    transactions::utils::check_tx_status,
};

// Updated original function
pub async fn create_and_send_swap_transaction(
    simulate_or_send: SendOrSimulate,
    chain: ChainType,
    transaction_infos: SwapPathResult,
) -> Result<()> {
    // Get priority fee from global service
    use crate::fees::priority_fees::get_global_fee_service;

    let profit_lamports = transaction_infos.result as u64;
    let priority_fee = match get_global_fee_service() {
        Ok(service) => {
            match service.get_priority_fee(profit_lamports).await {
                Ok(fee) => fee,
                Err(e) => {
                    error!("Failed to get priority fee: {:?}, using default", e);
                    10_000 // Fallback
                }
            }
        }
        Err(e) => {
            error!("Fee service not available: {:?}, using default", e);
            10_000 // Fallback
        }
    };

    // Use the new function with calculated fee
    create_and_send_swap_transaction_with_fee(
        simulate_or_send,
        chain,
        transaction_infos,
        priority_fee,
    )
    .await
}

/// Create and send swap transaction with explicit priority fee
pub async fn create_and_send_swap_transaction_with_fee(
    simulate_or_send: SendOrSimulate,
    chain: ChainType,
    transaction_infos: SwapPathResult,
    priority_fee_microlamports: u64,
) -> Result<()> {
    info!(
        "🔄 Create swap transaction with priority fee: {} microlamports ({:.6} SOL)",
        priority_fee_microlamports,
        priority_fee_microlamports as f64 / 1e9
    );

    let env = Env::new();
    let rpc_url = if chain.clone() == ChainType::Mainnet {
        &env.rpc_url_tx
    } else {
        &env.devnet_rpc_url
    };
    let rpc_client = NonBlockingRpcClient::new(rpc_url.to_string()); // Changed to NonBlockingRpcClient

    let payer: Keypair =
        read_keypair_file(env.payer_keypair_path.clone()).expect("Wallet keypair file not found");
    info!("💳 Wallet {:#?}", payer.pubkey());

    info!("🆔 Create/Send Swap instruction....");
    // Construct Swap instructions
    let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000); // Default CU limit
    let compute_budget_instruction_details = InstructionDetails {
        instruction: compute_budget_ix,
        details: "Compute Budget Instruction".to_string(),
        market: None,
    };

    let priority_fees_ix =
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee_microlamports);
    let priority_fees_instruction_details = InstructionDetails {
        instruction: priority_fees_ix,
        details: format!(
            "Set priority fees: {} microlamports",
            priority_fee_microlamports
        ),
        market: None,
    };

    let swaps_construct_instructions: Vec<InstructionDetails> =
        construct_transaction(transaction_infos.clone()).await;
    // Order: Priority Fee, Compute Budget, Swap Instructions
    let mut swap_instructions_details: Vec<InstructionDetails> = vec![
        priority_fees_instruction_details,
        compute_budget_instruction_details,
    ];
    swap_instructions_details.extend(swaps_construct_instructions);

    if swap_instructions_details
        .iter()
        .skip(2)
        .all(|id| id.instruction.accounts.is_empty() && id.instruction.data.is_empty())
    {
        // Check if actual swap instructions are empty
        error!("Error in create_transaction(), zero actual swap instructions constructed");
        return Ok(());
    }

    // Keep the accounts which are not in previously crafted LUT tables
    let mut lut_addresses: Vec<Pubkey> = Vec::new();
    for si_detail in swap_instructions_details.iter() {
        // Iterate over details
        if let Some(market_addr) = si_detail.market.as_ref().map(|m| m.address) {
            match get_lut_address_for_market(market_addr, false) {
                Ok((have_lut_address, Some(lut_address_val))) => {
                    if have_lut_address && !lut_addresses.contains(&lut_address_val) {
                        info!("LUT address {} pushed!", &lut_address_val);
                        lut_addresses.push(lut_address_val);
                    } else if !have_lut_address {
                        error!("❌ No LUT address configured for the market {:?}, though get_lut_address_for_market indicated one should exist.", market_addr);
                    }
                }
                Ok((_, None)) => {
                    error!(
                        "❌ No LUT address found for the market {:?}, the tx can revert...",
                        market_addr
                    );
                }
                Err(e) => {
                    error!(
                        "❌ Error getting LUT address for market {:?}: {:?}",
                        market_addr, e
                    );
                }
            }
        } else {
            info!(
                "Skip get LUT table for non swap instruction: {:?}",
                si_detail.details
            );
        }
    }

    let si_details_log: Vec<String> = swap_instructions_details
        .iter()
        .map(|instruc_details| instruc_details.details.clone())
        .collect();
    info!("📋 Swap instructions Details: {:#?}", si_details_log);
    // info!("Swap instructions : {:?}", swap_instructions_details); // Can be very verbose

    //Get previously crafted LUT address
    let mut vec_address_lut: Vec<AddressLookupTableAccount> = Vec::new();

    for lut_address in lut_addresses.iter() {
        // Iterate over references
        match rpc_client.get_account(lut_address).await { // Changed to await
            Ok(raw_lut_account) => match AddressLookupTable::deserialize(&raw_lut_account.data) {
                Ok(address_lookup_table) => {
                    let address_lookup_table_account = AddressLookupTableAccount {
                        key: *lut_address,
                        addresses: address_lookup_table.addresses.to_vec(),
                    };
                    info!(
                        "Address in lookup_table {}: {} addresses",
                        lut_address,
                        address_lookup_table_account.addresses.len()
                    );
                    vec_address_lut.push(address_lookup_table_account);
                }
                Err(e) => {
                    error!("❌ Failed to deserialize LUT {}: {:?}", lut_address, e);
                }
            },
            Err(e) => {
                error!("❌ Failed to get LUT account {}: {:?}", lut_address, e);
            }
        }
    }

    let mut instructions_for_tx: Vec<Instruction> = swap_instructions_details
        .iter()
        .map(|instruc_details| instruc_details.instruction.clone())
        .collect();

    let commitment_config = CommitmentConfig::confirmed();
    let latest_blockhash = match rpc_client.get_latest_blockhash_with_commitment(commitment_config).await
    {
        Ok((hash, _)) => hash,
        Err(e) => {
            error!("❌ Error in get latest blockhash: {:?}", e);
            return Err(e.into());
        }
    };

    let compiled_message = v0::Message::try_compile(
        &payer.pubkey(),
        &instructions_for_tx,
        &vec_address_lut,
        latest_blockhash,
    )?;

    let tx_to_simulate = VersionedTransaction::try_new(
        VersionedMessage::V0(compiled_message.clone()), // Clone for simulation
        &[&payer],
    )?;

    //Simulate
    let sim_config = RpcSimulateTransactionConfig {
        sig_verify: true, // Verify signatures, important for catching issues early
        commitment: Some(commitment_config),
        replace_recent_blockhash: true, // Recommended for simulation
        ..RpcSimulateTransactionConfig::default()
    };

    let sim_result = match rpc_client.simulate_transaction_with_config(&tx_to_simulate, sim_config).await
    {
        Ok(response) => response.value,
        Err(e) => {
            error!("❌ Simulation RPC call failed: {:?}", e);
            return Err(e.into());
        }
    };

    let logs_simulation = sim_result.logs.unwrap_or_default();
    if sim_result.err.is_some() {
        error!("❌ Simulate Transaction Error: {:#?}", sim_result.err);
        error!("📜 Simulation Logs: {:#?}", logs_simulation);
        return Ok(()); // Or Err if simulation failure should halt
    } else {
        info!("🧾 Simulate Tx Logs: {:#?}", logs_simulation);
    }

    let consumed_cus = sim_result.units_consumed.unwrap_or(1_400_000); // Use a default if None
    info!("🔢 Simulated CU Consumption: {}", consumed_cus);

    // Update instructions with the dynamic priority fee and adjusted compute units
    // Ensure instructions_for_tx vector has at least 2 elements before indexing
    if instructions_for_tx.len() >= 2 {
        instructions_for_tx[0] =
            ComputeBudgetInstruction::set_compute_unit_price(priority_fee_microlamports);
        instructions_for_tx[1] =
            ComputeBudgetInstruction::set_compute_unit_limit(consumed_cus as u32);
    } else {
        error!("❌ Instructions vector is too short to update compute budget and priority fee. Rebuilding.");
        // This case should ideally not happen if initial setup is correct
        let new_priority_ix =
            ComputeBudgetInstruction::set_compute_unit_price(priority_fee_microlamports);
        let new_cu_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(consumed_cus as u32);
        let original_swaps: Vec<Instruction> = swap_instructions_details
            .iter()
            .skip(2)
            .map(|id| id.instruction.clone())
            .collect();
        instructions_for_tx = vec![new_priority_ix, new_cu_limit_ix];
        instructions_for_tx.extend(original_swaps);
    }

    //Send transaction
    if simulate_or_send == SendOrSimulate::Send {
        let transaction_send_config: RpcSendTransactionConfig = RpcSendTransactionConfig {
            skip_preflight: true,
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            encoding: Some(UiTransactionEncoding::Base58),
            max_retries: Some(env.max_retries.unwrap_or(5)), // Use env var or default
            min_context_slot: None,
        };

        let new_payer_for_send: Keypair = read_keypair_file(&env.payer_keypair_path)
            .expect("Wallet keypair file not found for send");

        let latest_blockhash_for_send =
            match rpc_client.get_latest_blockhash_with_commitment(commitment_config).await {
                Ok((hash, _)) => hash,
                Err(e) => {
                    error!("❌ Error in get latest blockhash for send: {:?}", e);
                    return Err(e.into());
                }
            };

        let message_to_send = VersionedMessage::V0(v0::Message::try_compile(
            &new_payer_for_send.pubkey(),
            &instructions_for_tx, // Use the updated instructions
            &vec_address_lut,
            latest_blockhash_for_send,
        )?);

        let versioned_tx_to_send =
            VersionedTransaction::try_new(message_to_send, &[&new_payer_for_send])?;

        info!("📡 Sending transaction with RPC: {}", rpc_client.url());

        let non_blocking_rpc_client_send =
            solana_client::nonblocking::rpc_client::RpcClient::new(env.rpc_url_tx.clone());
        let arc_rpc_client_send = Arc::new(non_blocking_rpc_client_send);

        let connection_cache_send = if env.rpc_url_tx.starts_with("http") {
            ConnectionCache::new("connection_cache_cli_program_http_send")
        } else {
            ConnectionCache::new_quic("connection_cache_cli_program_quic_send", 1)
        };

        let _signer_send = new_payer_for_send;

        let _iteration_number_send = env.send_retry_count.unwrap_or(3); // Use env var or default

        if let ConnectionCache::Quic(cache_send) = connection_cache_send {
            match solana_client::nonblocking::tpu_client::TpuClient::new_with_connection_cache(
                arc_rpc_client_send.clone(),
                &env.wss_rpc_url, // Ensure WSS URL is correctly configured
                TpuClientConfig::default(),
                cache_send,
            )
            .await
            {
                Ok(_tpu_client) => {
                    // For versioned transactions, we need to use the RPC client directly
                    // send_and_confirm_transactions_in_parallel doesn't support versioned transactions yet
                    match arc_rpc_client_send
                        .send_transaction_with_config(
                            &versioned_tx_to_send,
                            transaction_send_config,
                        )
                        .await
                    {
                        Ok(signature) => {
                            info!(
                                "✅ Transaction sent via TPU/RPC with signature: {}",
                                signature
                            );
                            // Optionally confirm the transaction
                                    match arc_rpc_client_send.confirm_transaction(&signature).await {
                                        Ok(_) => info!("✅ Transaction confirmed!"),
                                        Err(e) => error!("❌ Failed to confirm transaction: {:?}", e),
                                    }
                                } // Closes Ok(signature) arm of inner match
                                Err(rpc_e) => { // Added Err arm for inner match
                                    error!("❌ Failed to send transaction via TPU/RPC: {:?}", rpc_e);
                                }
                            } // Closes inner match (send_transaction_with_config)
                        } // Closes Ok(tpu_client) arm of outer match
                        Err(e) => { // Err arm for TpuClient::new_with_connection_cache match
                            error!(
                                "❌ Failed to create TPU client: {:?}. Falling back to RPC send.",
                                e
                            );
                            // Fallback to RPC send if TPU client creation fails
                            match arc_rpc_client_send
                                .send_transaction_with_config(
                                    &versioned_tx_to_send,
                                    transaction_send_config,
                                )
                                .await
                            {
                                Ok(signature) => info!(
                                    "✅ Transaction sent via RPC fallback with signature: {}",
                                    signature
                                ),
                                Err(rpc_e) => error!(
                                    "❌ Failed to send transaction via RPC fallback: {:?}",
                                    rpc_e
                                ),
                            } // Closes fallback match
                        } // Closes Err(e) arm of outer match
                    } // Closes outer match (TpuClient::new_with_connection_cache)
                } else {
            warn!(
                "⚠️ Using basic RPC send_transaction for non-QUIC endpoint. Consider QUIC for TPU."
            );
            match arc_rpc_client_send
                .send_transaction_with_config(&versioned_tx_to_send, transaction_send_config)
                .await
            {
                Ok(signature) => info!("✅ Transaction sent via RPC with signature: {}", signature),
                Err(e) => error!("❌ Failed to send transaction via RPC: {:?}", e),
            }
        }
    }
    Ok(())
}

pub async fn create_ata_extendlut_transaction(
    chain: ChainType,
    simulate_or_send: SendOrSimulate,
    transaction_infos: SwapPathResult,
    lut_address: Pubkey,
    tokens: Vec<Pubkey>,
) -> Result<()> {
    info!("🔄 Create ATA/Extend LUT transaction.... ");

    let env = Env::new();
    let rpc_url = if chain.clone() == ChainType::Mainnet {
        &env.rpc_url_tx
    } else {
        &env.devnet_rpc_url
    };
    let rpc_client = NonBlockingRpcClient::new(rpc_url.to_string()); // Changed to NonBlockingRpcClient

    let payer: Keypair =
        read_keypair_file(env.payer_keypair_path.clone()).expect("Wallet keypair file not found");
    info!("💳 Wallet {:#?}", payer.pubkey());

    let mut vec_pda_instructions: Vec<Instruction> = Vec::new();

    //Create Pda/Ata accounts
    for token in tokens {
        let pda_user_token = get_associated_token_address(&payer.pubkey(), &token);
        match rpc_client.get_account(&pda_user_token).await { // Changed to await
            Ok(_account) => {
                // Changed variable name
                info!("🟢 PDA for {} already exist !", token);
            }
            Err(_error) => {
                // Changed variable name
                info!("👷‍♂️ PDA creation for {}...", token);
                let create_pda_instruction = create_associated_token_account(
                    &payer.pubkey(),
                    &payer.pubkey(),
                    &token,
                    &spl_token::id(),
                );
                vec_pda_instructions.push(create_pda_instruction);
            }
        }
    }

    // Create the extend LUT instructions
    let mut swap_instructions_for_lut: Vec<InstructionDetails> =
        construct_transaction(transaction_infos.clone()).await; // Cloned

    let mut instructions_to_extend_lut: Vec<InstructionDetails> = Vec::new();
    for instruction_detail in swap_instructions_for_lut.iter_mut() {
        // Iterate mutably
        if let Some(market_info) = instruction_detail.market.as_ref() {
            let market_addr = market_info.address;
            match get_lut_address_for_market(market_addr, false) {
                Ok((lut_exist, Some(_))) if lut_exist => {
                    // Check if lut_address is Some
                    info!("🟢 Lookup already exist for {} !", market_addr);
                    // No need to remove, just don't add to extend_instructions
                }
                _ => {
                    info!("👷‍♂️ Will attempt to extend lookup for: {}", market_addr);
                    if !instruction_detail.instruction.accounts.is_empty() {
                        instructions_to_extend_lut.push(instruction_detail.clone());
                        // Clone if adding
                    }
                }
            }
        }
    }

    if instructions_to_extend_lut.is_empty() && vec_pda_instructions.is_empty() {
        info!("➡️ No ATA creation or LUT extension needed for this transaction set.");
        return Ok(());
    }

    let mut vec_details_extend_instructions_final: Vec<InstructionDetails> = Vec::new();

    // Only take the first relevant instruction that needs LUT extension if multiple exist,
    // as typically one extend_lookup_table instruction is sent per transaction.
    if let Some(instr_to_extend) = instructions_to_extend_lut.first() {
        if !instr_to_extend.instruction.accounts.is_empty() {
            let accounts_to_add: Vec<Pubkey> = instr_to_extend
                .instruction
                .accounts
                .iter()
                .map(|acc_meta| acc_meta.pubkey)
                .collect();
            if !accounts_to_add.is_empty() {
                let extend_instruction = extend_lookup_table(
                    lut_address,
                    payer.pubkey(),
                    Some(payer.pubkey()),
                    accounts_to_add,
                );
                vec_details_extend_instructions_final.push(InstructionDetails {
                    instruction: extend_instruction,
                    details: format!(
                        "Extend LUT {} for market {:?}",
                        lut_address,
                        instr_to_extend.market.as_ref().map(|m| m.address)
                    ),
                    market: instr_to_extend.market.clone(),
                });
                info!(
                    "Extend LUT instruction prepared for market: {:?}",
                    instr_to_extend.market.as_ref().map(|m| m.address)
                );
            }
        }
    }

    let compute_budget_ix_lut = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000); // Default
    let priority_fees_ix_lut =
        ComputeBudgetInstruction::set_compute_unit_price(env.priority_fee_lut.unwrap_or(100)); // Use env var or default

    let mut vec_all_instructions_lut: Vec<Instruction> =
        vec![priority_fees_ix_lut, compute_budget_ix_lut];
    vec_all_instructions_lut.extend(vec_pda_instructions.clone());
    vec_all_instructions_lut.extend(
        vec_details_extend_instructions_final
            .iter()
            .map(|id| id.instruction.clone()),
    );

    if vec_all_instructions_lut.len() <= 2 {
        // Only CU and priority fee
        info!("➡️ No actual ATA creation or LUT extension instructions to send.");
        return Ok(());
    }

    let commitment_config_lut = CommitmentConfig::confirmed();
    let latest_blockhash_lut =
        match rpc_client.get_latest_blockhash_with_commitment(commitment_config_lut).await { // Changed to await
            Ok((hash, _)) => hash,
            Err(e) => {
                error!("❌ Error in get latest blockhash for LUT tx: {:?}", e);
                return Err(e.into());
            }
        };

    let txn_simulate_lut_msg = v0::Message::try_compile(
        &payer.pubkey(),
        &vec_all_instructions_lut,
        &[], // LUT creation/extension doesn't use existing LUTs in its own tx
        latest_blockhash_lut,
    )?;
    let txn_simulate_lut =
        VersionedTransaction::try_new(VersionedMessage::V0(txn_simulate_lut_msg), &[&payer])?;

    //Simulate
    let sim_config_lut = RpcSimulateTransactionConfig {
        sig_verify: true,
        commitment: Some(commitment_config_lut),
        replace_recent_blockhash: true,
        ..RpcSimulateTransactionConfig::default()
    };

    let sim_result = match rpc_client.simulate_transaction_with_config(&txn_simulate_lut, sim_config_lut).await // Changed to await
    {
        Ok(response) => response.value,
        Err(e) => {
                error!("❌ LUT/ATA Simulation RPC call failed: {:?}", e);
                return Err(e.into());
            }
        };

    if sim_result.err.is_some() {
        error!(
            "❌ Get out! Simulate Error for LUT/ATA Tx: {:#?}",
            sim_result.err
        );
        info!(
            "📜 LUT/ATA Simulation Logs on error: {:#?}",
            sim_result.logs
        );
        return Ok(());
    } else {
        info!("🧾 Simulate Tx Ata/Extend Logs: {:#?}", sim_result.logs);
    }

    let result_cu_lut = sim_result.units_consumed.unwrap_or(200_000); // Default for ATA/LUT
    info!("🔢 Computed Units for LUT/ATA Tx: {}", result_cu_lut);

    // Update CU limit based on simulation
    vec_all_instructions_lut[1] =
        ComputeBudgetInstruction::set_compute_unit_limit(result_cu_lut as u32);
    // Priority fee is already set from env var or default

    //Send transaction
    if simulate_or_send == SendOrSimulate::Send {
        let transaction_config_lut: RpcSendTransactionConfig = RpcSendTransactionConfig {
            skip_preflight: false, // Preflight for ATA/LUT can be useful
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            encoding: Some(UiTransactionEncoding::Base58),
            max_retries: Some(env.max_retries_lut.unwrap_or(5)), // Separate retry for LUT
            min_context_slot: None,
        };

        let new_payer_lut: Keypair = read_keypair_file(&env.payer_keypair_path)
            .expect("Wallet keypair file not found for LUT send");

        let latest_blockhash_lut_send =
            match rpc_client.get_latest_blockhash_with_commitment(commitment_config_lut).await {
                Ok((hash, _)) => hash,
                Err(e) => {
                    error!("❌ Error in get latest blockhash for LUT send: {:?}", e);
                    return Err(e.into());
                }
            };

        let message_lut_send = VersionedMessage::V0(v0::Message::try_compile(
            &new_payer_lut.pubkey(),
            &vec_all_instructions_lut,
            &[], // No LUTs for this tx itself
            latest_blockhash_lut_send,
        )?);
        let txn_lut_to_send = VersionedTransaction::try_new(message_lut_send, &[&new_payer_lut])?;

        info!(
            "📡 Sending LUT/ATA transaction via RPC: {}",
            rpc_client.url()
        );

        // For LUT/ATA, direct RPC send is often sufficient and simpler than TPU
        match rpc_client.send_transaction_with_config(&txn_lut_to_send, transaction_config_lut).await {
            Ok(signature) => {
                info!("✅ LUT/ATA Transaction sent with signature: {}", signature);
                // Optionally, confirm and then write to LUT cache
                if check_tx_status(commitment_config_lut, chain, signature).await? {
                    for details_instruction_written in vec_details_extend_instructions_final {
                        if let Some(market_info_written) = details_instruction_written.market {
                            match write_lut_for_market(
                                market_info_written.address,
                                lut_address,
                                false,
                            ) {
                                Ok(_) => info!(
                                    "Successfully wrote LUT info for market {}",
                                    market_info_written.address
                                ),
                                Err(e) => error!(
                                    "Failed to write LUT info for market {}: {:?}",
                                    market_info_written.address, e
                                ),
                            }
                        }
                    }
                    info!("✅ Ata/Extend transaction is well executed and confirmed.");
                } else {
                    error!("❌ Ata/Extend transaction sent but not confirmed or failed.");
                }
            }
            Err(e) => error!("❌ Failed to send LUT/ATA transaction via RPC: {:?}", e),
        }
    }

    Ok(())
}

pub async fn construct_transaction(transaction_infos: SwapPathResult) -> Vec<InstructionDetails> {
    let mut swap_instructions: Vec<InstructionDetails> = Vec::new();

    for (i, route_sim) in transaction_infos
        .route_simulations
        .clone()
        .iter()
        .enumerate()
    {
        match route_sim.dex_label {
            DexLabel::Meteora => {
                let swap_params: SwapParametersMeteora = SwapParametersMeteora {
                    lb_pair: from_str(transaction_infos.route_simulations[i].pool_address.as_str())
                        .unwrap(),
                    amount_in: transaction_infos.route_simulations[i].amount_in,
                    swap_for_y: transaction_infos.route_simulations[i].token_0to1,
                    input_token: from_str(transaction_infos.route_simulations[i].token_in.as_str())
                        .unwrap(),
                    output_token: from_str(
                        transaction_infos.route_simulations[i].token_out.as_str(),
                    )
                    .unwrap(),
                    minimum_amount_out: transaction_infos.route_simulations[i]
                        .estimated_amount_out
                        .parse()
                        .unwrap_or_else(|_| {
                            error!(
                                "Failed to parse minimum_amount_out for Meteora: {}",
                                transaction_infos.route_simulations[i].estimated_amount_out
                            );
                            0 // Fallback, consider how to handle this better
                        }),
                };
                let result = construct_meteora_instructions(swap_params.clone()).await;
                if result.is_empty() {
                    // Check if empty
                    error!("Error in Meteora Instruction construction: returned empty");
                    return Vec::new(); // Return empty Vec
                }
                for instruction in result {
                    swap_instructions.push(instruction);
                }
            }
            DexLabel::Raydium => {
                let swap_params: SwapParametersRaydium = SwapParametersRaydium {
                    pool: from_str(transaction_infos.route_simulations[i].pool_address.as_str())
                        .unwrap(),
                    input_token_mint: from_str(route_sim.token_in.as_str()).unwrap(),
                    output_token_mint: from_str(route_sim.token_out.as_str()).unwrap(),
                    amount_in: transaction_infos.route_simulations[i].amount_in,
                    swap_for_y: transaction_infos.route_simulations[i].token_0to1, // This might need adjustment based on Raydium's API for which token is A/B
                    min_amount_out: transaction_infos.route_simulations[i]
                        .estimated_amount_out
                        .parse()
                        .unwrap_or_else(|_| {
                            error!(
                                "Failed to parse minimum_amount_out for Raydium: {}",
                                transaction_infos.route_simulations[i].estimated_amount_out
                            );
                            0
                        }),
                };
                let result = construct_raydium_instructions(swap_params).await;
                if result.is_empty() {
                    error!("Error in Raydium Instruction construction: returned empty");
                    return Vec::new();
                }
                for instruction in result {
                    swap_instructions.push(instruction);
                }
            }
            DexLabel::RaydiumClmm => {
                info!("Creating RaydiumClmm transaction...");
                // Basic implementation for RaydiumClmm
                let input_token = from_str(route_sim.token_in.as_str()).unwrap();
                let output_token = from_str(route_sim.token_out.as_str()).unwrap();
                let pool_address = from_str(transaction_infos.route_simulations[i].pool_address.as_str()).unwrap();
                
                // Create associated token accounts if needed
                let payer_pubkey = solana_sdk::pubkey::Pubkey::new_unique(); // This would be replaced with actual payer
                let _associated_token_in = get_associated_token_address(&payer_pubkey, &input_token);
                let _associated_token_out = get_associated_token_address(&payer_pubkey, &output_token);
                
                // Create placeholder instruction
                let placeholder_instruction = solana_sdk::system_instruction::transfer(
                    &payer_pubkey,
                    &payer_pubkey,
                    0 // No actual transfer
                );
                
                // Add market information and details
                let market_info = MarketInfos {
                    dex_label: DexLabel::RaydiumClmm,
                    address: pool_address,
                };
                
                swap_instructions.push(InstructionDetails {
                    instruction: placeholder_instruction,
                    details: format!(
                        "RaydiumClmm swap: {} to {}, amount: {}", 
                        route_sim.token_in, 
                        route_sim.token_out, 
                        transaction_infos.route_simulations[i].amount_in
                    ),
                    market: Some(market_info),
                });
                
                info!("Added RaydiumClmm transaction (placeholder)");
            }
            DexLabel::OrcaWhirlpools => {
                let swap_params: SwapParametersOrcaWhirpools = SwapParametersOrcaWhirpools {
                    whirpools: from_str(
                        transaction_infos.route_simulations[i].pool_address.as_str(),
                    )
                    .unwrap(),
                    input_token: from_str(route_sim.token_in.as_str()).unwrap(),
                    output_token: from_str(route_sim.token_out.as_str()).unwrap(),
                    amount_in: transaction_infos.route_simulations[i].amount_in,
                    minimum_amount_out: transaction_infos.route_simulations[i]
                        .estimated_amount_out
                        .parse()
                        .unwrap_or_else(|_| {
                            error!(
                                "Failed to parse minimum_amount_out for Orca: {}",
                                transaction_infos.route_simulations[i].estimated_amount_out
                            );
                            0
                        }),
                };
                let result = construct_orca_whirpools_instructions(swap_params).await;
                if result.is_empty() {
                    error!("Error in Orca_Whirpools Instruction construction: returned empty");
                    return Vec::new();
                }
                for instruction in result {
                    swap_instructions.push(instruction);
                }
            }
            DexLabel::Orca => {
                info!("Creating Orca transaction...");
                // Basic implementation for Orca
                let input_token = from_str(route_sim.token_in.as_str()).unwrap();
                let output_token = from_str(route_sim.token_out.as_str()).unwrap();
                let pool_address = from_str(transaction_infos.route_simulations[i].pool_address.as_str()).unwrap();
                
                // Create associated token accounts if needed
                let payer_pubkey = solana_sdk::pubkey::Pubkey::new_unique(); // This would be replaced with actual payer
                let _associated_token_in = get_associated_token_address(&payer_pubkey, &input_token);
                let _associated_token_out = get_associated_token_address(&payer_pubkey, &output_token);
                
                // Create placeholder instruction for Orca
                let placeholder_instruction = solana_sdk::system_instruction::transfer(
                    &payer_pubkey,
                    &payer_pubkey,
                    0 // No actual transfer
                );
                
                // Add market information and details
                let market_info = MarketInfos {
                    dex_label: DexLabel::Orca,
                    address: pool_address,
                };
                
                swap_instructions.push(InstructionDetails {
                    instruction: placeholder_instruction,
                    details: format!(
                        "Orca swap: {} to {}, amount: {}", 
                        route_sim.token_in, 
                        route_sim.token_out, 
                        transaction_infos.route_simulations[i].amount_in
                    ),
                    market: Some(market_info),
                });
                
                info!("Added Orca transaction (placeholder)");
            }
        }
    }
    return swap_instructions;
}

pub async fn create_lut(chain: ChainType) -> Result<()> {
    info!("🆔 Create/Send LUT transaction....");
    let env = Env::new();
    let rpc_url = if chain == ChainType::Mainnet {
        env.rpc_url.clone()
    } else {
        env.devnet_rpc_url.clone()
    }; // Cloned
    let rpc_client: RpcClient = RpcClient::new(rpc_url);
    let payer: Keypair =
        read_keypair_file(&env.payer_keypair_path).expect("Wallet keypair file not found"); // Removed clone

    //Create Address Lookup Table (LUT acronym)
    let slot = rpc_client
        .get_slot_with_commitment(CommitmentConfig::finalized())
        .expect("Error in get slot");
    let (create_lut_instruction, lut_address) = create_lookup_table(
        payer.pubkey(),
        payer.pubkey(),
        slot.saturating_sub(200), // Use saturating_sub
    );

    let latest_blockhash_lut = rpc_client
        .get_latest_blockhash()
        .expect("Error in get latest blockhash for LUT");

    let txn_lut_msg = v0::Message::try_compile(
        &payer.pubkey(),
        &[create_lut_instruction.clone()],
        &[],
        latest_blockhash_lut,
    )?;
    let txn_lut = VersionedTransaction::try_new(VersionedMessage::V0(txn_lut_msg), &[&payer])?;

    let transaction_config_lut_send: RpcSendTransactionConfig = RpcSendTransactionConfig {
        skip_preflight: false, // Good to preflight LUT creation
        preflight_commitment: Some(CommitmentLevel::Confirmed),
        ..RpcSendTransactionConfig::default()
    };

    let signature =
        rpc_client.send_transaction_with_config(&txn_lut, transaction_config_lut_send)?; // Propagate error

    if chain == ChainType::Devnet {
        info!(
            "https://explorer.solana.com/tx/{}?cluster=devnet",
            signature
        );
    } else {
        info!("https://explorer.solana.com/tx/{}", signature);
    }
    let commitment_config_check = CommitmentConfig::confirmed();
    let tx_confirmed = check_tx_status(commitment_config_check, chain, signature).await?;
    if tx_confirmed {
        info!("✅ Address LUT {} is well created", lut_address);
        // Optionally write this new LUT to a general cache if needed immediately
        // For now, it's up to create_ata_extendlut_transaction to associate it with markets
    } else {
        error!(
            "❌ Address LUT {} creation failed or not confirmed",
            lut_address
        );
    }

    Ok(())
}

pub async fn is_available_lut(chain: ChainType, lut_address: Pubkey) -> Result<bool> {
    info!(
        "🚚 Check if LUT address {} is available to extend...",
        lut_address
    );
    let env = Env::new();
    let rpc_url = if chain == ChainType::Mainnet {
        env.rpc_url.clone()
    } else {
        env.devnet_rpc_url.clone()
    };
    let rpc_client: RpcClient = RpcClient::new(rpc_url);
    // let payer: Keypair = read_keypair_file(&env.payer_keypair_path).expect("Wallet keypair file not found"); // Payer not needed for read

    let raw_lut_account = rpc_client.get_account(&lut_address)?;
    let address_lookup_table = AddressLookupTable::deserialize(&raw_lut_account.data)?;

    const MAX_ADDRESSES_PER_LUT: usize = 256; // Solana limit
    let current_len = address_lookup_table.addresses.len();
    // Typically, you might want to leave some buffer, e.g., if current_len < MAX_ADDRESSES_PER_LUT - 5
    if current_len < MAX_ADDRESSES_PER_LUT - env.lut_buffer_count.unwrap_or(10) as usize {
        // Use env var or default buffer
        info!(
            "LUT {} has {} addresses, available for extension.",
            lut_address, current_len
        );
        return Ok(true);
    } else {
        info!(
            "LUT {} has {} addresses, considered full or close to full.",
            lut_address, current_len
        );
        return Ok(false);
    }
}

pub fn get_lut_address_for_market(market: Pubkey, is_test: bool) -> Result<(bool, Option<Pubkey>)> {
    let path_str = if is_test {
        "src/transactions/cache/lut_addresses_test.json"
    } else {
        "src/transactions/cache/lut_addresses.json"
    };
    let path = Path::new(path_str);

    if !path.exists() {
        info!("LUT cache file {} does not exist.", path_str);
        return Ok((false, None)); // File doesn't exist, so no LUT for this market yet
    }

    let file_read = OpenOptions::new().read(true).open(path)?;
    let reader = BufReader::new(file_read);
    let lut_file_data: VecLUTFile = match serde_json::from_reader(reader) {
        Ok(data) => data,
        Err(e) if e.is_eof() => {
            // Handle empty file case
            info!("LUT cache file {} is empty.", path_str);
            VecLUTFile { value: Vec::new() }
        }
        Err(e) => {
            error!("Failed to parse LUT cache file {}: {:?}", path_str, e);
            return Err(e.into());
        }
    };

    let market_str = market.to_string();
    let found_entry = lut_file_data
        .value
        .iter()
        .find(|entry| entry.market == market_str);

    match found_entry {
        Some(value) => {
            match from_str(&value.lut_address) {
                Ok(pubkey) => Ok((true, Some(pubkey))),
                Err(e) => {
                    error!(
                        "Failed to parse Pubkey from LUT address string '{}' for market {}: {:?}",
                        value.lut_address, market_str, e
                    );
                    Ok((false, None)) // Treat as if not found if address is invalid
                }
            }
        }
        None => Ok((false, None)),
    }
}

pub fn write_lut_for_market(market: Pubkey, lut_address: Pubkey, is_test: bool) -> Result<()> {
    let path_str = if is_test {
        "src/transactions/cache/lut_addresses_test.json"
    } else {
        "src/transactions/cache/lut_addresses.json"
    };
    let path = Path::new(path_str);

    let mut lut_file_data: VecLUTFile = if path.exists() {
        let file_read = OpenOptions::new().read(true).open(path)?;
        let reader = BufReader::new(file_read);
        match serde_json::from_reader(reader) {
            Ok(data) => data,
            Err(e) if e.is_eof() => VecLUTFile { value: Vec::new() }, // Handle empty file
            Err(e) => return Err(e.into()),
        }
    } else {
        VecLUTFile { value: Vec::new() }
    };

    let market_str = market.to_string();
    let lut_address_str = lut_address.to_string();

    // Check if market already exists, update if so, otherwise add new
    if let Some(entry) = lut_file_data
        .value
        .iter_mut()
        .find(|e| e.market == market_str)
    {
        info!(
            "Updating LUT address for market {} from {} to {}",
            market_str, entry.lut_address, lut_address_str
        );
        entry.lut_address = lut_address_str;
    } else {
        info!(
            "Adding new LUT entry for market {}: {}",
            market_str, lut_address_str
        );
        lut_file_data.value.push(LUTFile {
            market: market_str,
            lut_address: lut_address_str,
        });
    }

    // Ensure directory exists
    if let Some(parent_dir) = path.parent() {
        std::fs::create_dir_all(parent_dir)?;
    }

    let file_write = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    let mut writer = BufWriter::new(file_write);
    serde_json::to_writer_pretty(&mut writer, &lut_file_data)?; // Use to_writer_pretty for readability
    writer.flush()?; // Ensure all data is written
    info!("Data written to '{}' successfully.", path_str);

    Ok(())
}

////////////////////

#[derive(Debug, Clone)]
pub struct InstructionDetails {
    pub instruction: Instruction,
    pub details: String,
    pub market: Option<MarketInfos>,
}
#[derive(Debug, Clone)]
pub struct MarketInfos {
    pub dex_label: DexLabel,
    pub address: Pubkey,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VecLUTFile {
    pub value: Vec<LUTFile>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LUTFile {
    pub market: String,
    pub lut_address: String,
}

pub enum TransactionType {
    CreateLUT,
    CreateSwap,
}
#[derive(PartialEq, Clone, Debug)] // Added Debug
pub enum SendOrSimulate {
    Simulate,
    Send,
}

#[derive(PartialEq, Clone, Debug)] // Added Debug
pub enum ChainType {
    Mainnet,
    Devnet,
}
