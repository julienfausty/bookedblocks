use ndarray::Array2;

fn gaussian_kernel_1d(value: f64, deviation: &f64, mean: &f64) -> f64 {
    (1.0 / (deviation * (2.0 * std::f64::consts::PI).sqrt()))
        * (-(value - mean).powi(2) / (2.0 * deviation.powi(2))).exp()
}

/// method for gaussian kernel density estimation from a source sample onto regular 1D grid
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
    let kernel_bloom = (5.0 * deviation / step).round() as i64;

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

fn gaussian_kernel_2d(values: (f64, f64), deviations: &(f64, f64), means: &(f64, f64)) -> f64 {
    (1.0 / (deviations.0 * deviations.1 * 2.0 * std::f64::consts::PI))
        * ((-1.0 / 2.0)
            * (((values.0 - means.0) / deviations.0).powi(2)
                + ((values.1 - means.1) / deviations.1).powi(2)))
        .exp()
}

/// method for gaussian kernel density estimation from a source sample onto regular 2D grid
pub fn splat_2d(
    ranges: (&(f64, f64), &(f64, f64)),
    grid_sizes: (usize, usize),
    source: Vec<(f64, f64, f64)>,
) -> Array2<f64> {
    let mut support = Array2::zeros(grid_sizes);

    if source.len() == 0 {
        return support;
    }

    if (ranges.0.0 == ranges.0.1) || (ranges.1.0 == ranges.1.1) {
        support += 1.0;
        return support;
    }

    let grid_sizes = (support.shape()[0].clone(), support.shape()[1].clone());
    let deviations = (
        (ranges.0.1 - ranges.0.0) / (2.0 * (source.len() as f64).sqrt()),
        (ranges.1.1 - ranges.1.0) / (2.0 * (source.len() as f64).sqrt()),
    );
    let steps = (
        (ranges.0.1 - ranges.0.0) / (grid_sizes.0 as f64),
        (ranges.1.1 - ranges.1.0) / (grid_sizes.1 as f64),
    );
    let kernel_blooms = (
        (5.0 * deviations.0 / steps.0).round() as i64,
        (5.0 * deviations.1 / steps.1).round() as i64,
    );

    let influence = |value: (f64, f64)| {
        let grid_point = (
            ((value.0 - ranges.0.0) / steps.0).round() as i64,
            ((value.1 - ranges.1.0) / steps.1).round() as i64,
        );
        let mut extents = (
            (
                grid_point.0 - kernel_blooms.0,
                grid_point.0 + kernel_blooms.0 + 1,
            ),
            (
                grid_point.1 - kernel_blooms.1,
                grid_point.1 + kernel_blooms.1 + 1,
            ),
        );

        if extents.0.0 < 0 {
            extents.0.0 = 0;
        }
        if extents.1.0 < 0 {
            extents.1.0 = 0;
        }

        if extents.0.1 > grid_sizes.0 as i64 {
            extents.0.1 = grid_sizes.0 as i64;
        }
        if extents.1.1 > grid_sizes.1 as i64 {
            extents.1.1 = grid_sizes.1 as i64;
        }

        extents
    };

    for (key0, key1, value) in source.into_iter() {
        let splat_extents = influence((key0, key1));

        for index0 in (splat_extents.0.0)..(splat_extents.0.1) {
            for index1 in (splat_extents.1.0)..(splat_extents.1.1) {
                match support.get_mut((index0 as usize, index1 as usize)) {
                    Some(val) => {
                        *val += value
                            * gaussian_kernel_2d(
                                (
                                    steps.0 * (index0 as f64) + ranges.0.0,
                                    steps.1 * (index1 as f64) + ranges.1.0,
                                ),
                                &deviations,
                                &(key0, key1),
                            )
                    }
                    None => (),
                }
            }
        }
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

    #[test]
    fn test_splat_2d_empty_source() {
        let splatted = splat_2d((&(0.0, 1.0), &(0.0, 1.0)), (20, 10), Vec::new());

        assert!(splatted.shape()[0] == 20);
        assert!(splatted.shape()[1] == 10);

        for splat_val in splatted.into_iter() {
            assert!(splat_val == 0.0);
        }
    }

    #[test]
    fn test_splat_2d_compact_horizontal_range() {
        let splatted = splat_2d((&(0.0, 0.0), &(0.0, 1.0)), (20, 10), vec![(0.0, 0.0, 0.0)]);

        assert!(splatted.shape()[0] == 20);
        assert!(splatted.shape()[1] == 10);

        for splat_val in splatted.into_iter() {
            assert!(splat_val == 1.0);
        }
    }

    #[test]
    fn test_splat_2d_compact_vertical_range() {
        let splatted = splat_2d((&(0.0, 1.0), &(1.0, 1.0)), (20, 10), vec![(0.0, 0.0, 0.0)]);

        assert!(splatted.shape()[0] == 20);
        assert!(splatted.shape()[1] == 10);

        for splat_val in splatted.into_iter() {
            assert!(splat_val == 1.0);
        }
    }

    #[test]
    fn test_splat_2d_one_source() {
        let splatted = splat_2d((&(0.0, 1.0), &(0.0, 1.0)), (10, 20), vec![(0.5, 0.5, 1.0)]);

        assert!(splatted.shape()[0] == 10);
        assert!(splatted.shape()[1] == 20);

        for i_grid in 0..10 {
            for j_grid in 0..20 {
                assert!(
                    (splatted.get((i_grid, j_grid)).unwrap()
                        - gaussian_kernel_2d(
                            (i_grid as f64 / 10.0, j_grid as f64 / 20.0),
                            &(0.5, 0.5),
                            &(0.5, 0.5)
                        ))
                    .abs()
                        < TOLERANCE
                );
            }
        }
    }

    #[test]
    fn test_splat_2d_volume() {
        let splatted = splat_2d((&(1.0, 2.0), &(1.0, 2.0)), (10, 20), vec![(1.5, 1.5, 0.25)]);

        assert!(splatted.shape()[0] == 10);
        assert!(splatted.shape()[1] == 20);

        for i_grid in 0..10 {
            for j_grid in 0..20 {
                assert!(
                    (splatted.get((i_grid, j_grid)).unwrap()
                        - 0.25
                            * gaussian_kernel_2d(
                                (i_grid as f64 / 10.0, j_grid as f64 / 20.0),
                                &(0.5, 0.5),
                                &(0.5, 0.5)
                            ))
                    .abs()
                        < TOLERANCE
                );
            }
        }
    }

    #[test]
    fn test_splat_2d_multiple_sources() {
        let splatted = splat_2d(
            (&(1.0, 2.0), &(-1.0, 0.0)),
            (10, 20),
            vec![
                (1.0, -1.0, 1.2),
                (1.5, -0.5, 0.25),
                (1.5, 0.0, 0.7),
                (2.0, 0.0, 1.4),
            ],
        );

        assert!(splatted.shape()[0] == 10);
        assert!(splatted.shape()[1] == 20);

        let deviation = 0.25;
        let kernel = |grid_point: (f64, f64)| -> f64 {
            1.2 * gaussian_kernel_2d(grid_point, &(deviation, deviation), &(0.0, 0.0))
                + 0.25 * gaussian_kernel_2d(grid_point, &(deviation, deviation), &(0.5, 0.5))
                + 0.7 * gaussian_kernel_2d(grid_point, &(deviation, deviation), &(0.5, 1.0))
                + 1.4 * gaussian_kernel_2d(grid_point, &(deviation, deviation), &(1.0, 1.0))
        };

        for i_grid in 0..10 {
            for j_grid in 0..20 {
                assert!(
                    (splatted.get((i_grid, j_grid)).unwrap()
                        - kernel((i_grid as f64 / 10.0, j_grid as f64 / 20.0)))
                    .abs()
                        < TOLERANCE
                );
            }
        }
    }
}
