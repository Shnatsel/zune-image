/*
 * Copyright (c) 2023.
 *
 * This software is free software;
 *
 * You can redistribute it or modify it under terms of the MIT, Apache License or Zlib license
 */

use zune_core::bit_depth::BitType;
use zune_core::log::trace;
use zune_image::errors::ImageErrors;
use zune_image::image::Image;
use zune_image::traits::OperationsTrait;

use crate::traits::NumOps;

/// Gamma adjust an image
///
/// This currently only supports 8 and 16 bit depth images since it applies an optimization
/// that works for those depths.
///
/// This operation is internally multithreaded, where supported
#[derive(Default)]
pub struct Gamma {
    value: f32
}

impl Gamma {
    /// Create a new gamma correction operation.
    ///
    /// # Arguments
    /// value: Ranges typical range is from 0.8-2.3
    #[must_use]
    pub fn new(value: f32) -> Gamma {
        Gamma { value }
    }
}

impl OperationsTrait for Gamma {
    fn get_name(&self) -> &'static str {
        "Gamma Correction"
    }

    fn execute_impl(&self, image: &mut Image) -> Result<(), ImageErrors> {
        let max_value = image.get_depth().max_value();

        let depth = image.get_depth();
        #[cfg(not(feature = "threads"))]
        {
            trace!("Running gamma correction in single threaded mode");

            for channel in image.get_channels_mut(false) {
                match depth.bit_type() {
                    BitType::U16 => {
                        gamma(channel.reinterpret_as_mut::<u16>()?, self.value, max_value)
                    }
                    BitType::U8 => {
                        gamma(channel.reinterpret_as_mut::<u8>()?, self.value, max_value)
                    }
                    d => {
                        return Err(ImageErrors::ImageOperationNotImplemented(
                            self.get_name(),
                            d
                        ))
                    }
                }
            }
        }
        #[cfg(feature = "threads")]
        {
            trace!("Running gamma correction in multithreaded mode");

            std::thread::scope(|s| {
                let mut errors = vec![];
                for channel in image.get_channels_mut(false) {
                    let t = s.spawn(|| match depth.bit_type() {
                        BitType::U16 => {
                            gamma(channel.reinterpret_as_mut::<u16>()?, self.value, max_value);
                            Ok(())
                        }
                        BitType::U8 => {
                            gamma(channel.reinterpret_as_mut::<u8>()?, self.value, max_value);
                            Ok(())
                        }
                        d => {
                            return Err(ImageErrors::ImageOperationNotImplemented(
                                self.get_name(),
                                d
                            ))
                        }
                    });
                    errors.push(t);
                }
                errors
                    .into_iter()
                    .map(|x| x.join().unwrap())
                    .collect::<Result<Vec<()>, ImageErrors>>()
            })?;
        }
        Ok(())
    }
    fn supported_types(&self) -> &'static [BitType] {
        &[BitType::U8, BitType::U16]
    }
}

#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::needless_range_loop,
    clippy::cast_precision_loss
)]
pub fn gamma<T>(pixels: &mut [T], value: f32, max_value: u16)
where
    T: Copy + NumOps<T> + Default
{
    // build a lookup table which we use for gamma correction in the next stage
    // it is faster to do it this way as calling pow in the inner loop is slow

    // must be inclusive so that 65535 and 255 are covered
    let mut lut = vec![T::default(); usize::from(max_value) + 1];

    let max_usize = usize::from(max_value);
    let max_value = max_value as f32;
    let value_inv = 1.0 / max_value;
    // optimizer hint to remove bounds check, these values should be
    // powers of two, currently we support 255 and 65535
    assert!(lut.len().is_power_of_two());
    let lut_mask = lut.len() - 1;

    for x in 0..=max_usize {
        let pixel_f32 = (x as f32) * value_inv;
        let mut new_pix_val = max_value * pixel_f32.powf(value);

        if new_pix_val > max_value {
            new_pix_val = max_value;
        }

        lut[x & lut_mask] = T::from_f32(new_pix_val);
    }
    // now do gamma correction
    for px in pixels {
        *px = lut[(*px).to_usize() & lut_mask];
    }
}

#[cfg(feature = "benchmarks")]
#[cfg(test)]
mod benchmarks {
    extern crate test;

    use crate::gamma::gamma;

    #[bench]
    fn gamma_bench(b: &mut test::Bencher) {
        let width = 800;
        let height = 800;
        let dimensions = width * height;

        let mut c1 = vec![0_u16; dimensions];

        b.iter(|| {
            gamma(&mut c1, 2.3, 255);
        });
    }
}
