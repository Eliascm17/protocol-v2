pub fn convert_to_number(big_number: u64, precision: u128) -> f64 {
    if big_number == 0 {
        return 0.0;
    }
    big_number as f64 / precision as f64
}
