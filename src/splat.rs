fn gaussian_kernel_1d(price: f64, deviation: &f64, mean: &f64) -> f64 {
    (1.0 / (deviation * (2.0 * std::f64::consts::PI).sqrt()))
        * (-(price - mean).powi(2) / deviation.powi(2)).exp()
}

pub fn splat_1d(range: &(f64, f64), grid_size: usize, source: Vec<(f64, f64)>) -> Vec<f64> {
    let mut support = vec![0.0; grid_size];

    if source.len() == 0 {
        return support;
    }

    if range.0 == range.1 {
        support.fill(1.0);
        return support;
    }

    let grid_size = support.len().clone();
    let deviation = (range.1 - range.0) / (2.0 * source.len() as f64);
    let step = (range.1 - range.0) / (grid_size as f64);
    let kernel_bloom = (2.5 * deviation / step).round() as i64;

    let influence = |value: f64| {
        let grid_point = ((value - range.0) / step).round() as i64;
        let mut extent = (grid_point - kernel_bloom, grid_point + kernel_bloom + 1);
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
                    value * gaussian_kernel_1d(step * (index as f64) + range.0, &deviation, &key)
            })
            .collect::<Vec<_>>();
    }

    support
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE: f64 = 1e-2;

    #[test]
    fn test_splat_1d_empty_source() {
        let splatted = splat_1d(&(0.0, 1.0), 10, Vec::new());

        assert!(splatted.len() == 10);

        for splat_val in splatted.into_iter() {
            assert!(splat_val == 0.0);
        }
    }

    #[test]
    fn test_splat_1d_compact_range() {
        let splatted = splat_1d(&(0.0, 0.0), 10, vec![(0.0, 0.0), (1.0, 1.0)]);

        assert!(splatted.len() == 10);

        for splat_val in splatted.into_iter() {
            assert!(splat_val == 1.0);
        }
    }

    #[test]
    fn test_splat_1d_one_source() {
        let splatted = splat_1d(&(0.0, 1.0), 10, vec![(0.5, 1.0)]);

        assert!(splatted.len() == 10);

        for i_grid in 0..10 {
            assert!(
                (splatted[i_grid] - gaussian_kernel_1d((i_grid as f64) / 10.0, &0.5, &0.5)).abs()
                    < TOLERANCE
            );
        }
    }

    #[test]
    fn test_splat_1d_volume() {
        let splatted = splat_1d(&(0.0, 1.0), 20, vec![(0.5, 0.3)]);

        assert!(splatted.len() == 20);

        for i_grid in 0..20 {
            assert!(
                (splatted[i_grid] - 0.3 * gaussian_kernel_1d((i_grid as f64) / 20.0, &0.5, &0.5))
                    .abs()
                    < TOLERANCE
            );
        }
    }

    #[test]
    fn test_splat_1d_multiple_sources() {
        let splatted = splat_1d(
            &(0.0, 1.0),
            50,
            vec![(0.0, 0.4), (0.2, 0.3), (0.4, 1.0), (0.6, 0.8), (1.0, 0.2)],
        );

        assert!(splatted.len() == 50);

        let kernel = |price: f64| -> f64 {
            0.4 * gaussian_kernel_1d(price, &0.1, &0.0)
                + 0.3 * gaussian_kernel_1d(price, &0.1, &0.2)
                + 1.0 * gaussian_kernel_1d(price, &0.1, &0.4)
                + 0.8 * gaussian_kernel_1d(price, &0.1, &0.6)
                + 0.2 * gaussian_kernel_1d(price, &0.1, &1.0)
        };

        for i_grid in 0..50 {
            assert!((splatted[i_grid] - kernel((i_grid as f64) / 50.0)).abs() < TOLERANCE);
        }
    }
}
