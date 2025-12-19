fn gaussian_kernel(price: f64, deviation: &f64, mean: &f64) -> f64 {
    (1.0 / (deviation * (2.0 * std::f64::consts::PI).sqrt()))
        * (-(price - mean).powi(2) / deviation.powi(2)).exp()
}

pub fn splat_1D(range: &(f64, f64), grid_size: usize, source: Vec<(f64, f64)>) -> Vec<f64> {
    let mut support = Vec::with_capacity(grid_size);

    if source.len() == 0 {
        support.fill(0.0);
        return support;
    }

    if range.0 == range.1 {
        support.fill(1.0);
        return support;
    }

    let grid_size = support.len().clone();
    let deviation = (grid_size as f64) / (2.0 * source.len() as f64);
    let step = (range.1 - range.0) / (grid_size as f64);
    let extent = (2.0 * deviation / step).round() as i64;

    let influence = |value: f64| {
        let grid_point = ((value - range.0) / step).round() as i64;
        let mut extent = (grid_point - extent, grid_point + extent + 1);
        if extent.0 < 0 {
            extent.0 = 0;
        }
        if extent.1 > grid_size as i64 {
            extent.1 = grid_size as i64;
        }
        extent
    };

    for (key, value) in source.into_iter() {
        let splat_extent = influence(key);

        let _ = ((splat_extent.0)..(splat_extent.1))
            .map(|index| {
                support[index as usize] +=
                    value * gaussian_kernel(step * (index as f64) + range.0, &deviation, &key)
            })
            .collect::<Vec<_>>();
    }

    support
}
