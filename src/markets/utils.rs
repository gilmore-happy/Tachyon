pub fn to_pair_string(mint_a: String, mint_b: String) -> String {
    if mint_a < mint_b {
        format!("{}/{}", mint_a, mint_b)
    } else {
        format!("{}/{}", mint_b, mint_a)
    }
}
