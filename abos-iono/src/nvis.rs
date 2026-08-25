pub fn select_nvis_frequency(fo_f2: f64, time_of_day: f64) -> f64 {
    let diurnal_factor = if time_of_day >= 6.0 && time_of_day <= 18.0 {
        0.85
    } else {
        0.70
    };
    fo_f2 * diurnal_factor
}
