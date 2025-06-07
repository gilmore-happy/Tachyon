use super::meteora::{fetch_data_meteora, MeteoraDEX};
use super::orca::{fetch_data_orca, OrcaDex};
use super::orca_whirpools::{fetch_data_orca_whirpools, OrcaDexWhirpools};
use super::raydium::{fetch_data_raydium, RaydiumDEX};
use super::raydium_clmm::{fetch_data_raydium_clmm, RaydiumClmmDEX};
use super::types::{Dex, DexLabel};

use log::info;

pub async fn load_all_pools(refecth_api: bool) -> Vec<Dex> {
    if refecth_api {
        info!("Fetching all markets from APIs concurrently...");

        let (
            _raydium_clmm_result,
            _orca_result,
            _orca_whirpools_result,
            _raydium_result,
            _meteora_result,
        ) = tokio::join!(
            fetch_data_raydium_clmm(),
            fetch_data_orca(),
            fetch_data_orca_whirpools(),
            fetch_data_raydium(),
            fetch_data_meteora()
        );

        info!("✅ Finished fetching all markets");
    }

    let dex1 = Dex::new(DexLabel::RaydiumClmm);
    let dex_raydium_clmm = RaydiumClmmDEX::new(dex1);
    let dex2 = Dex::new(DexLabel::Orca);
    let dex_orca = OrcaDex::new(dex2);
    let dex3 = Dex::new(DexLabel::OrcaWhirlpools);
    let dex_orca_whirpools = OrcaDexWhirpools::new(dex3);
    let dex4 = Dex::new(DexLabel::Raydium);
    let dex_raydium = RaydiumDEX::new(dex4);
    let dex5 = Dex::new(DexLabel::Meteora);
    let dex_meteora = MeteoraDEX::new(dex5);

    let mut results: Vec<Dex> = Vec::new();
    results.push(dex_raydium_clmm.dex);
    results.push(dex_orca.dex);
    results.push(dex_orca_whirpools.dex);
    results.push(dex_raydium.dex);
    results.push(dex_meteora.dex);
    return results;
}
