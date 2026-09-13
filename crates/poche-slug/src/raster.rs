// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Renderer-neutral filled RGBA rasterization built on the checked Slug
//! outline and directional-band contract.

use crate::{
    DEFAULT_BAND_SIZE_FONT_UNITS, DirectionalBands, GlyphGeometry, Point, SlugError, SlugFont,
    build_directional_bands, coverage_banded,
};

const RASTER_PIXELS_PER_EM: f32 = 96.0;
const RASTER_PADDING_PIXELS: u32 = 2;
const MAXIMUM_RASTER_WIDTH: f32 = 1_024.0;

struct RasterGlyph {
    cursor: f32,
    geometry: GlyphGeometry,
    bands: DirectionalBands,
}

/// A tightly packed, transparent RGBA8 image plus its dimensions in font
/// design units. Renderer adapters decide how large that design rectangle is
/// in their own world.
#[derive(Clone, Debug, PartialEq)]
pub struct SlugRgbaRaster {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Row-major RGBA8 pixels.
    pub bytes: Vec<u8>,
    /// Raster width expressed in font design units.
    pub design_width: f32,
    /// Raster height expressed in font design units.
    pub design_height: f32,
}

/// Rasterize filled text through the same analytic coverage implementation
/// used by Poche's native spatial mirror.
///
/// # Errors
///
/// Returns a [`SlugError`] when glyph geometry, directional bands, bounds, or
/// the resulting allocation are invalid.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "validated finite positive Slug bounds are explicitly rasterized into bounded pixel extents; raster coordinates are intentionally sampled as f32"
)]
pub fn rasterize_text_rgba(
    font: &SlugFont<'_>,
    text: &str,
    color: [u8; 3],
) -> Result<SlugRgbaRaster, SlugError> {
    let metrics = font.metrics();
    let mut glyphs = Vec::with_capacity(text.chars().count());
    let mut cursor = 0.0_f32;
    let mut minimum_x = 0.0_f32;
    let mut maximum_x = 0.0_f32;
    let mut minimum_y = metrics.descender;
    let mut maximum_y = metrics.ascender;
    for character in text.chars() {
        let geometry = font.glyph_geometry(character)?;
        minimum_x = minimum_x.min(cursor + geometry.bounds.min_x);
        maximum_x = maximum_x.max(cursor + geometry.bounds.max_x);
        minimum_y = minimum_y.min(geometry.bounds.min_y);
        maximum_y = maximum_y.max(geometry.bounds.max_y);
        let bands = build_directional_bands(
            &geometry.curves,
            geometry.bounds,
            DEFAULT_BAND_SIZE_FONT_UNITS,
        )?;
        let advance = geometry.advance;
        glyphs.push(RasterGlyph {
            cursor,
            geometry,
            bands,
        });
        cursor += advance;
        maximum_x = maximum_x.max(cursor);
    }

    let content_width = (maximum_x - minimum_x).max(1.0);
    let content_height = (maximum_y - minimum_y).max(1.0);
    if !content_width.is_finite() || !content_height.is_finite() {
        return Err(SlugError::InvalidBounds);
    }
    let nominal_pixels_per_font_unit = RASTER_PIXELS_PER_EM / f32::from(metrics.units_per_em);
    let available_width = MAXIMUM_RASTER_WIDTH - 2.0 * RASTER_PADDING_PIXELS as f32;
    let pixels_per_font_unit = nominal_pixels_per_font_unit
        .min(available_width / content_width)
        .max(f32::EPSILON);
    let padding_units = RASTER_PADDING_PIXELS as f32 / pixels_per_font_unit;
    let canvas_minimum_x = minimum_x - padding_units;
    let canvas_maximum_y = maximum_y + padding_units;
    let width = ((content_width * pixels_per_font_unit).ceil() as u32)
        .saturating_add(RASTER_PADDING_PIXELS * 2)
        .max(1);
    let height = ((content_height * pixels_per_font_unit).ceil() as u32)
        .saturating_add(RASTER_PADDING_PIXELS * 2)
        .max(1);
    let width_usize =
        usize::try_from(width).map_err(|_| SlugError::CountOverflow("Slug raster width"))?;
    let height_usize =
        usize::try_from(height).map_err(|_| SlugError::CountOverflow("Slug raster height"))?;
    let byte_count = width_usize
        .checked_mul(height_usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(SlugError::CountOverflow("Slug RGBA raster"))?;
    let mut bytes = vec![0_u8; byte_count];

    for pixel_y in 0..height {
        let sample_y = canvas_maximum_y - (pixel_y as f32 + 0.5) / pixels_per_font_unit;
        for pixel_x in 0..width {
            let sample_x = canvas_minimum_x + (pixel_x as f32 + 0.5) / pixels_per_font_unit;
            let mut coverage = 0.0_f32;
            for glyph in &glyphs {
                let local_x = sample_x - glyph.cursor;
                if local_x < glyph.geometry.bounds.min_x
                    || local_x > glyph.geometry.bounds.max_x
                    || sample_y < glyph.geometry.bounds.min_y
                    || sample_y > glyph.geometry.bounds.max_y
                {
                    continue;
                }
                coverage = coverage.max(coverage_banded(
                    &glyph.geometry.curves,
                    &glyph.bands,
                    Point::new(local_x, sample_y),
                    pixels_per_font_unit,
                )?);
            }
            let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
            if alpha == 0 {
                continue;
            }
            let pixel_index = usize::try_from(pixel_y * width + pixel_x)
                .map_err(|_| SlugError::CountOverflow("Slug raster pixel index"))?
                * 4;
            bytes[pixel_index..pixel_index + 3].copy_from_slice(&color);
            bytes[pixel_index + 3] = alpha;
        }
    }

    Ok(SlugRgbaRaster {
        width,
        height,
        bytes,
        design_width: width as f32 / pixels_per_font_unit,
        design_height: height as f32 / pixels_per_font_unit,
    })
}
