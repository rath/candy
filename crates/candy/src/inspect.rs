//! `candy inspect`: stored image metadata plus statistics of the decoded pixels.
use anyhow::{Context, Result};
use candy_core::PixelStats;
use candy_macos::Metadata;
use serde_json::{Value, json};
use std::{fmt::Write, path::Path};

pub fn run(path: &Path, json: bool) -> Result<()> {
    let size = std::fs::metadata(path)
        .with_context(|| format!("Cannot read {}", path.display()))?
        .len();
    let metadata = candy_macos::inspect(path)?;
    // Metadata comes from the headers; statistics need the full decode, which
    // fails for formats Candy cannot edit and for images over the pixel limit.
    let pixels = candy_macos::load(path)
        .map(|image| candy_core::pixel_stats(&image))
        .map_err(|error| format!("{error:#}"));
    let report = Report {
        path,
        size,
        metadata,
        pixels,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report.json())?);
    } else {
        print!("{}", report.text());
    }
    Ok(())
}

struct Report<'a> {
    path: &'a Path,
    size: u64,
    metadata: Metadata,
    pixels: Result<PixelStats, String>,
}

impl Report<'_> {
    fn text(&self) -> String {
        let m = &self.metadata;
        let mut lines: Vec<(&str, String)> = Vec::new();
        let size = if self.size < 1000 {
            format!("{} bytes", self.size)
        } else {
            format!("{} bytes, {}", self.size, human_size(self.size))
        };
        lines.push(("File", format!("{} ({size})", self.path.display())));
        lines.push((
            "Format",
            match m.format {
                Some(name) => format!("{name} ({})", m.uti),
                None => format!("{} (not editable in Candy)", m.uti),
            },
        ));
        let mut dimensions = format!("{} × {}", m.width, m.height);
        if m.orientation != 1 {
            let (width, height) = m.display_dimensions();
            write!(
                dimensions,
                ", orientation {} → shown as {width} × {height}",
                m.orientation
            )
            .unwrap();
        }
        lines.push(("Dimensions", dimensions));
        let mut color: Vec<String> = Vec::new();
        color.extend(m.color_model.clone());
        color.extend(m.depth.map(|depth| format!("{depth} bits per channel")));
        color.push(
            if m.has_alpha {
                "alpha channel"
            } else {
                "no alpha channel"
            }
            .to_owned(),
        );
        if m.is_float {
            color.push("floating point".to_owned());
        }
        if m.is_indexed {
            color.push("indexed".to_owned());
        }
        lines.push(("Color", color.join(", ")));
        if let Some(profile) = &m.profile {
            lines.push(("Profile", profile.clone()));
        }
        if let Some((x, y)) = m.dpi {
            lines.push(("DPI", format!("{x} × {y}")));
        }
        lines.push(("Images", m.images.to_string()));
        match &self.pixels {
            Ok(stats) => {
                let total = stats.total();
                let share = |count: u64| {
                    if total == 0 {
                        count.to_string()
                    } else {
                        format!("{count} ({:.1}%)", count as f64 * 100.0 / total as f64)
                    }
                };
                lines.push(("Pixels", "8-bit sRGB after orientation".to_owned()));
                lines.push(("  Opaque", share(stats.opaque)));
                lines.push(("  Translucent", share(stats.translucent)));
                lines.push(("  Transparent", share(stats.transparent)));
                lines.push(("  Mean alpha", format!("{:.1}", stats.mean_alpha)));
                lines.push((
                    "  Average",
                    stats
                        .average
                        .map_or_else(|| "none (fully transparent)".to_owned(), hex),
                ));
                lines.push(("  Colors", format!("{} distinct RGB", stats.colors)));
            }
            Err(reason) => lines.push(("Pixels", format!("not decoded: {reason}"))),
        }
        let width = lines
            .iter()
            .map(|(label, _)| label.len())
            .max()
            .unwrap_or(0)
            + 2;
        lines
            .iter()
            .map(|(label, value)| format!("{label:<width$}{value}\n"))
            .collect()
    }

    fn json(&self) -> Value {
        let m = &self.metadata;
        let (display_width, display_height) = m.display_dimensions();
        let (pixels, pixels_error) = match &self.pixels {
            Ok(stats) => (
                json!({
                    "opaque": stats.opaque,
                    "translucent": stats.translucent,
                    "transparent": stats.transparent,
                    "mean_alpha": stats.mean_alpha,
                    "average": stats.average.map(hex),
                    "colors": stats.colors,
                }),
                Value::Null,
            ),
            Err(reason) => (Value::Null, json!(reason)),
        };
        json!({
            "path": self.path.display().to_string(),
            "size_bytes": self.size,
            "format": m.format,
            "uti": m.uti,
            "width": m.width,
            "height": m.height,
            "orientation": m.orientation,
            "display_width": display_width,
            "display_height": display_height,
            "color_model": m.color_model,
            "depth": m.depth,
            "has_alpha": m.has_alpha,
            "is_float": m.is_float,
            "is_indexed": m.is_indexed,
            "profile": m.profile,
            "dpi": m.dpi.map(|(x, y)| [x, y]),
            "images": m.images,
            "pixels": pixels,
            "pixels_error": pixels_error,
        })
    }
}

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo() -> Metadata {
        Metadata {
            uti: "public.heic".to_owned(),
            format: Some("HEIC"),
            width: 4032,
            height: 3024,
            orientation: 6,
            color_model: Some("RGB".to_owned()),
            depth: Some(10),
            has_alpha: false,
            is_float: false,
            is_indexed: false,
            profile: Some("Display P3".to_owned()),
            dpi: Some((72.0, 72.0)),
            images: 1,
        }
    }

    fn stats() -> PixelStats {
        PixelStats {
            opaque: 4,
            translucent: 0,
            transparent: 0,
            average: Some([127, 106, 82]),
            mean_alpha: 255.0,
            colors: 3,
        }
    }

    #[test]
    fn text_report_aligns_values_in_one_column() {
        let report = Report {
            path: Path::new("photo.heic"),
            size: 2_431_221,
            metadata: photo(),
            pixels: Ok(stats()),
        };
        assert_eq!(
            report.text(),
            "File           photo.heic (2431221 bytes, 2.4 MB)\n\
             Format         HEIC (public.heic)\n\
             Dimensions     4032 × 3024, orientation 6 → shown as 3024 × 4032\n\
             Color          RGB, 10 bits per channel, no alpha channel\n\
             Profile        Display P3\n\
             DPI            72 × 72\n\
             Images         1\n\
             Pixels         8-bit sRGB after orientation\n\
             \x20 Opaque       4 (100.0%)\n\
             \x20 Translucent  0 (0.0%)\n\
             \x20 Transparent  0 (0.0%)\n\
             \x20 Mean alpha   255.0\n\
             \x20 Average      #7f6a52\n\
             \x20 Colors       3 distinct RGB\n"
        );
    }

    #[test]
    fn text_report_omits_unknown_fields_and_explains_undecoded_pixels() {
        let report = Report {
            path: Path::new("scan.tiff"),
            size: 512,
            metadata: Metadata {
                uti: "public.tiff".to_owned(),
                format: None,
                width: 10,
                height: 20,
                orientation: 1,
                color_model: None,
                depth: None,
                has_alpha: false,
                is_float: false,
                is_indexed: false,
                profile: None,
                dpi: None,
                images: 2,
            },
            pixels: Err("Supported image formats are PNG (found public.tiff)".to_owned()),
        };
        assert_eq!(
            report.text(),
            "File        scan.tiff (512 bytes)\n\
             Format      public.tiff (not editable in Candy)\n\
             Dimensions  10 × 20\n\
             Color       no alpha channel\n\
             Images      2\n\
             Pixels      not decoded: Supported image formats are PNG (found public.tiff)\n"
        );
    }

    #[test]
    fn json_report_uses_null_for_absent_values() {
        let report = Report {
            path: Path::new("photo.heic"),
            size: 2_431_221,
            metadata: photo(),
            pixels: Ok(stats()),
        };
        let value = report.json();
        assert_eq!(value["format"], "HEIC");
        assert_eq!(value["display_width"], 3024);
        assert_eq!(value["dpi"], json!([72.0, 72.0]));
        assert_eq!(value["has_alpha"], false);
        assert_eq!(value["is_float"], false);
        assert_eq!(value["pixels"]["average"], "#7f6a52");
        assert_eq!(value["pixels"]["colors"], 3);
        assert_eq!(value["pixels_error"], Value::Null);
        let failed = Report {
            pixels: Err("too large".to_owned()),
            ..report
        };
        let value = failed.json();
        assert_eq!(value["pixels"], Value::Null);
        assert_eq!(value["pixels_error"], "too large");
    }

    #[test]
    fn sizes_use_decimal_units_with_one_decimal() {
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1000), "1.0 KB");
        assert_eq!(human_size(2_431_221), "2.4 MB");
        assert_eq!(human_size(5_000_000_000_000), "5000.0 GB");
    }
}
