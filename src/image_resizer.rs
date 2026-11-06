//! PowerToys Image Resizer sizing, names and batch plan. Pixels are written by the Windows adapter.
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
pub const MAX_BATCH: usize = 1000;
const MAX_NAME: usize = 64;
const MAX_TEMPLATE: usize = 180;
const MAX_UNIQUIFIER: u32 = 10_000;
const AVOID: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum Fit {
    Fit = 0,
    Fill = 1,
    Stretch = 2,
}
impl Fit {
    pub fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            0 => Self::Fit,
            1 => Self::Fill,
            2 => Self::Stretch,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Fit => "Sığdır",
            Self::Fill => "Doldur",
            Self::Stretch => "Uzat",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Pixel = 0,
    Percent = 1,
    Inch = 2,
    Centimeter = 3,
}
impl Unit {
    pub fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            0 => Self::Pixel,
            1 => Self::Percent,
            2 => Self::Inch,
            3 => Self::Centimeter,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Pixel => "Piksel",
            Self::Percent => "Yüzde",
            Self::Inch => "İnç",
            Self::Centimeter => "Santimetre",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackEncoder {
    Jpeg,
    Png,
    Bmp,
    Gif,
    Tiff,
}
impl FallbackEncoder {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "jpeg" => Self::Jpeg,
            "png" => Self::Png,
            "bmp" => Self::Bmp,
            "gif" => Self::Gif,
            "tiff" => Self::Tiff,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Png => "png",
            Self::Bmp => "bmp",
            Self::Gif => "gif",
            Self::Tiff => "tiff",
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::Bmp => "BMP",
            Self::Gif => "GIF",
            Self::Tiff => "TIFF",
        }
    }
    fn encoder(self) -> Encoder {
        match self {
            Self::Jpeg => Encoder::Jpeg,
            Self::Png => Encoder::Png,
            Self::Bmp => Encoder::Bmp,
            Self::Gif => Encoder::Gif,
            Self::Tiff => Encoder::Tiff,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PngInterlace {
    Default = 0,
    On = 1,
    Off = 2,
}
impl PngInterlace {
    pub fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            0 => Self::Default,
            1 => Self::On,
            2 => Self::Off,
            _ => return None,
        })
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "Varsayılan",
            Self::On => "Açık",
            Self::Off => "Kapalı",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TiffCompression {
    Default = 0,
    None = 1,
    Ccitt3 = 2,
    Ccitt4 = 3,
    Lzw = 4,
    Rle = 5,
    Zip = 6,
}
impl TiffCompression {
    pub fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            0 => Self::Default,
            1 => Self::None,
            2 => Self::Ccitt3,
            3 => Self::Ccitt4,
            4 => Self::Lzw,
            5 => Self::Rle,
            6 => Self::Zip,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Varsayılan",
            Self::None => "Yok",
            Self::Ccitt3 => "CCITT3",
            Self::Ccitt4 => "CCITT4",
            Self::Lzw => "LZW",
            Self::Rle => "RLE",
            Self::Zip => "ZIP",
        }
    }
    pub fn next(self, step: i32) -> Self {
        let count = 7;
        let current = self as i32;
        let next = (current + step).rem_euclid(count);
        Self::from_index(next).unwrap_or(Self::Default)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoder {
    Jpeg,
    Png,
    Bmp,
    Gif,
    Tiff,
    Wmp,
}
impl Encoder {
    pub fn from_fallback(fallback: FallbackEncoder) -> Self {
        fallback.encoder()
    }
    pub fn default_extension(self) -> &'static str {
        match self {
            Self::Jpeg => ".jpg",
            Self::Png => ".png",
            Self::Bmp => ".bmp",
            Self::Gif => ".gif",
            Self::Tiff => ".tiff",
            Self::Wmp => ".wdp",
        }
    }
    pub fn supports(self, extension: &str) -> bool {
        let extension = extension.trim_start_matches('.').to_ascii_lowercase();
        matches!(
            (self, extension.as_str()),
            (Self::Jpeg, "jpg" | "jpeg" | "jpe")
                | (Self::Png, "png")
                | (Self::Bmp, "bmp" | "dib")
                | (Self::Gif, "gif")
                | (Self::Tiff, "tif" | "tiff")
                | (Self::Wmp, "wdp" | "jxr" | "hdp")
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    Jpeg,
    Png,
    Bmp,
    Gif,
    Tiff,
    Wmp,
    Other,
}
impl Container {
    pub fn native_encoder(self) -> Option<Encoder> {
        Some(match self {
            Self::Jpeg => Encoder::Jpeg,
            Self::Png => Encoder::Png,
            Self::Bmp => Encoder::Bmp,
            Self::Gif => Encoder::Gif,
            Self::Tiff => Encoder::Tiff,
            Self::Wmp => Encoder::Wmp,
            Self::Other => return None,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Size {
    pub name: String,
    pub fit: Fit,
    pub width: f64,
    pub height: f64,
    pub unit: Unit,
}
impl Size {
    fn preset(name: &str, width: f64, height: f64) -> Self {
        Self {
            name: name.into(),
            fit: Fit::Fit,
            width,
            height,
            unit: Unit::Pixel,
        }
    }
    pub fn parse_setting(value: &str) -> Option<Self> {
        let mut parts = value.split('|');
        let name = parts.next()?;
        let fit = Fit::from_index(parts.next()?.parse().ok()?)?;
        let width = parts.next()?.parse().ok()?;
        let height = parts.next()?.parse().ok()?;
        let unit = Unit::from_index(parts.next()?.parse().ok()?)?;
        if parts.next().is_some()
            || !valid_name(name)
            || !valid_extent(width)
            || !valid_extent(height)
        {
            return None;
        }
        Some(Self {
            name: name.into(),
            fit,
            width,
            height,
            unit,
        })
    }
    pub fn persist(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.name,
            self.fit as i32,
            quantity(self.width),
            quantity(self.height),
            self.unit as i32
        )
    }
    pub fn summary(&self) -> String {
        let name = display_name(&self.name);
        if self.fit != Fit::Stretch && self.unit == Unit::Percent {
            format!("{name} — {}% {}", quantity(self.width), self.fit.label())
        } else {
            format!(
                "{name} — {} × {} {} {}",
                quantity(self.width),
                quantity(self.height),
                self.unit.label(),
                self.fit.label()
            )
        }
    }
    fn has_auto(&self) -> bool {
        is_auto(self.width) || is_auto(self.height)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResizeSettings {
    pub sizes: Vec<Size>,
    pub selected: usize,
    pub custom: Size,
    pub shrink_only: bool,
    pub replace: bool,
    pub ignore_orientation: bool,
    pub remove_metadata: bool,
    pub jpeg_quality: i32,
    pub png: PngInterlace,
    pub tiff: TiffCompression,
    pub filename: String,
    pub keep_date: bool,
    pub fallback: FallbackEncoder,
}
impl Default for ResizeSettings {
    fn default() -> Self {
        Self {
            sizes: vec![
                Size::preset("small", 854.0, 480.0),
                Size::preset("medium", 1366.0, 768.0),
                Size::preset("large", 1920.0, 1080.0),
                Size::preset("phone", 320.0, 568.0),
            ],
            selected: 0,
            custom: Size {
                name: "custom".into(),
                fit: Fit::Fit,
                width: 1024.0,
                height: 640.0,
                unit: Unit::Pixel,
            },
            shrink_only: false,
            replace: false,
            ignore_orientation: true,
            remove_metadata: false,
            jpeg_quality: 90,
            png: PngInterlace::Default,
            tiff: TiffCompression::Default,
            filename: "%1 (%2)".into(),
            keep_date: false,
            fallback: FallbackEncoder::Jpeg,
        }
    }
}
impl ResizeSettings {
    pub fn parse_filename(value: &str) -> Option<String> {
        let trimmed = value.trim();
        if trimmed.is_empty()
            || trimmed.len() > MAX_TEMPLATE
            || trimmed.chars().any(|c| c.is_control())
        {
            return None;
        }
        Some(trimmed.to_string())
    }
    pub fn selected_size(&self) -> Size {
        if self.selected < self.sizes.len() {
            self.sizes[self.selected].clone()
        } else {
            self.custom.clone()
        }
    }
    pub fn choice_labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = self.sizes.iter().map(Size::summary).collect();
        labels.push("Özel".into());
        labels
    }
    pub fn append_settings(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = write!(
            out,
            "resize_shrink={}\nresize_replace={}\nresize_ignore_orientation={}\nresize_remove_metadata={}\nresize_jpeg={}\nresize_png={}\nresize_tiff={}\nresize_keep_date={}\nresize_encoder={}\nresize_filename={}\nresize_selected={}\nresize_custom={}\n",
            self.shrink_only,
            self.replace,
            self.ignore_orientation,
            self.remove_metadata,
            self.jpeg_quality,
            self.png as i32,
            self.tiff as i32,
            self.keep_date,
            self.fallback.as_str(),
            self.filename,
            self.selected,
            self.custom.persist()
        );
        for size in &self.sizes {
            let _ = writeln!(out, "resize_size={}", size.persist());
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceImage {
    pub width: u32,
    pub height: u32,
    pub dpi_x: f64,
    pub dpi_y: f64,
    pub container: Container,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramePlan {
    pub scaled_width: u32,
    pub scaled_height: u32,
    pub crop: Option<Crop>,
    pub output_width: u32,
    pub output_height: u32,
    pub unchanged: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanFault {
    Unreadable,
    Dimension,
    Name,
    TooMany,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedWrite {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub backup: Option<PathBuf>,
    pub replace: bool,
    pub encoder: Encoder,
    pub frame: FramePlan,
    pub container: Container,
    pub source_width: u32,
    pub source_height: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub enum PlanItem {
    Ready(PlannedWrite),
    Failed { source: PathBuf, fault: PlanFault },
}
pub fn display_name(name: &str) -> String {
    match name {
        "small" | "$small$" => "Küçük".into(),
        "medium" | "$medium$" => "Orta".into(),
        "large" | "$large$" => "Büyük".into(),
        "phone" | "$phone$" => "Telefon".into(),
        "custom" | "$custom$" => "Özel".into(),
        other => other.to_string(),
    }
}
pub fn calculate(
    image: &SourceImage,
    size: &Size,
    settings: &ResizeSettings,
) -> Result<FramePlan, PlanFault> {
    if image.width == 0 || image.height == 0 || size.width < 0.0 || size.height < 0.0 {
        return Err(PlanFault::Dimension);
    }
    let mut width = target_pixels(size.width, size.unit, image.width, image.dpi_x, size.fit);
    let mut height = target_pixels(
        if size.fit != Fit::Stretch && size.unit == Unit::Percent {
            size.width
        } else {
            size.height
        },
        size.unit,
        image.height,
        image.dpi_y,
        size.fit,
    );
    let can_swap = settings.ignore_orientation
        && !size.has_auto()
        && size.unit != Unit::Percent
        && width.is_finite()
        && height.is_finite();
    if can_swap {
        let input_landscape = image.width > image.height;
        let input_portrait = image.height > image.width;
        let target_landscape = width > height;
        let target_portrait = height > width;
        if (input_landscape && target_portrait) || (input_portrait && target_landscape) {
            std::mem::swap(&mut width, &mut height);
        }
    }
    let mut scale_x = axis_scale(width, image.width);
    let mut scale_y = axis_scale(height, image.height);
    match size.fit {
        Fit::Fit => {
            let scale = scale_x.min(scale_y);
            scale_x = scale;
            scale_y = scale;
        }
        Fit::Fill => {
            let scale = scale_x.max(scale_y);
            scale_x = scale;
            scale_y = scale;
        }
        Fit::Stretch => {}
    }
    if settings.shrink_only && size.unit != Unit::Percent {
        let enlarges = scale_x > 1.0 || scale_y > 1.0;
        let fill_crops = size.fit == Fit::Fill
            && width.is_finite()
            && height.is_finite()
            && (f64::from(image.width) > width || f64::from(image.height) > height);
        if enlarges || (scale_x == 1.0 && scale_y == 1.0 && !fill_crops) {
            return Ok(unchanged(image));
        }
    }
    let scaled_width = scaled_dimension(f64::from(image.width) * scale_x)?;
    let scaled_height = scaled_dimension(f64::from(image.height) * scale_y)?;
    if size.fit == Fit::Fill && width.is_finite() && height.is_finite() {
        let target_width = scaled_dimension(width)?;
        let target_height = scaled_dimension(height)?;
        if scaled_width > target_width || scaled_height > target_height {
            let crop = Crop {
                x: scaled_width.saturating_sub(target_width) / 2,
                y: scaled_height.saturating_sub(target_height) / 2,
                width: target_width,
                height: target_height,
            };
            return Ok(FramePlan {
                scaled_width,
                scaled_height,
                output_width: crop.width,
                output_height: crop.height,
                crop: Some(crop),
                unchanged: false,
            });
        }
    }
    Ok(FramePlan {
        scaled_width,
        scaled_height,
        crop: None,
        output_width: scaled_width,
        output_height: scaled_height,
        unchanged: false,
    })
}
pub fn plan_batch(
    files: &[PathBuf],
    settings: &ResizeSettings,
    images: &[Option<SourceImage>],
    mut exists: impl FnMut(&Path) -> bool,
) -> Vec<PlanItem> {
    if files.len() > MAX_BATCH {
        let source = files.first().cloned().unwrap_or_default();
        return vec![PlanItem::Failed {
            source,
            fault: PlanFault::TooMany,
        }];
    }
    let size = settings.selected_size();
    let mut reserved = HashSet::new();
    let mut seen = HashSet::new();
    files
        .iter()
        .zip(images.iter().chain(std::iter::repeat(&None)))
        .filter_map(|(path, image)| {
            let key = path_key(path);
            if !seen.insert(key) {
                return None;
            }
            Some(plan_one(
                path,
                *image,
                &size,
                settings,
                &mut exists,
                &mut reserved,
            ))
        })
        .collect()
}
pub fn can_copy_bytes(plan: &PlannedWrite, settings: &ResizeSettings) -> bool {
    plan.frame.unchanged
        && !settings.remove_metadata
        && plan.container.native_encoder() == Some(plan.encoder)
        && same_extension(&plan.source, &plan.destination)
        && match plan.encoder {
            Encoder::Png => settings.png == PngInterlace::Default,
            Encoder::Tiff => settings.tiff == TiffCompression::Default,
            Encoder::Jpeg | Encoder::Bmp | Encoder::Gif | Encoder::Wmp => true,
        }
}
pub fn output_encoder(container: Container, fallback: FallbackEncoder) -> Encoder {
    container
        .native_encoder()
        .unwrap_or_else(|| Encoder::from_fallback(fallback))
}
fn plan_one(
    path: &Path,
    image: Option<SourceImage>,
    size: &Size,
    settings: &ResizeSettings,
    exists: &mut impl FnMut(&Path) -> bool,
    reserved: &mut HashSet<String>,
) -> PlanItem {
    let Some(image) = image else {
        return PlanItem::Failed {
            source: path.to_path_buf(),
            fault: PlanFault::Unreadable,
        };
    };
    let frame = match calculate(&image, size, settings) {
        Ok(frame) => frame,
        Err(fault) => {
            return PlanItem::Failed {
                source: path.to_path_buf(),
                fault,
            };
        }
    };
    let encoder = output_encoder(image.container, settings.fallback);
    let extension = output_extension(path, encoder);
    let Some(directory) = path.parent() else {
        return PlanItem::Failed {
            source: path.to_path_buf(),
            fault: PlanFault::Name,
        };
    };
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("image");
    let formatted = match formatted_stem(
        stem,
        &display_name(&size.name),
        size,
        &frame,
        &settings.filename,
    ) {
        Some(name) => name,
        None => {
            return PlanItem::Failed {
                source: path.to_path_buf(),
                fault: PlanFault::Name,
            };
        }
    };
    let destination = if settings.replace {
        path.to_path_buf()
    } else {
        match unique_path(directory, &formatted, &extension, "", exists, reserved) {
            Some(path) => path,
            None => {
                return PlanItem::Failed {
                    source: path.to_path_buf(),
                    fault: PlanFault::Name,
                };
            }
        }
    };
    let backup = if settings.replace {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("image");
        match unique_path(
            directory,
            stem,
            &extension_of(path),
            ".bak",
            exists,
            reserved,
        ) {
            Some(path) => Some(path),
            None => {
                return PlanItem::Failed {
                    source: path.to_path_buf(),
                    fault: PlanFault::Name,
                };
            }
        }
    } else {
        None
    };
    PlanItem::Ready(PlannedWrite {
        source: path.to_path_buf(),
        destination,
        backup,
        replace: settings.replace,
        encoder,
        frame,
        container: image.container,
        source_width: image.width,
        source_height: image.height,
    })
}
fn formatted_stem(
    original: &str,
    size_name: &str,
    size: &Size,
    frame: &FramePlan,
    template: &str,
) -> Option<String> {
    let size_name = size_name.replace(['\\', '/'], "_");
    let formatted = apply_template(
        template,
        &[
            original,
            &size_name,
            &quantity(size.width),
            &quantity(size.height),
            &frame.output_width.to_string(),
            &frame.output_height.to_string(),
        ],
    );
    let mut stem: String = formatted
        .chars()
        .map(|c| match c {
            ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\\' | '/' => '_',
            other => other,
        })
        .collect();
    while stem.ends_with([' ', '.']) {
        stem.pop();
    }
    if stem.is_empty() {
        stem = "image".into();
    }
    if AVOID.contains(&stem.to_ascii_uppercase().as_str()) {
        stem.push('_');
    }
    if stem.len() > 240 {
        stem.truncate(240);
    }
    Some(stem)
}
fn apply_template(template: &str, values: &[&str; 6]) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '%'
            && let Some(digit) = chars.get(index + 1).and_then(|c| c.to_digit(10))
            && (1..=6).contains(&digit)
        {
            out.push_str(values[(digit - 1) as usize]);
            index += 2;
            continue;
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}
fn unique_path(
    directory: &Path,
    stem: &str,
    extension: &str,
    infix: &str,
    exists: &mut impl FnMut(&Path) -> bool,
    reserved: &mut HashSet<String>,
) -> Option<PathBuf> {
    let candidate = |suffix: &str| directory.join(format!("{stem}{suffix}{infix}{extension}"));
    let mut path = candidate("");
    let mut uniquifier = 1u32;
    while taken(&path, exists, reserved) {
        if uniquifier > MAX_UNIQUIFIER {
            return None;
        }
        path = candidate(&format!(" ({uniquifier})"));
        uniquifier += 1;
    }
    reserved.insert(path_key(&path));
    Some(path)
}
fn taken(path: &Path, exists: &mut impl FnMut(&Path) -> bool, reserved: &HashSet<String>) -> bool {
    reserved.contains(&path_key(path)) || exists(path)
}
fn path_key(path: &Path) -> String {
    path.to_string_lossy().to_ascii_lowercase()
}
fn output_extension(path: &Path, encoder: Encoder) -> String {
    let current = extension_of(path);
    let bare = current.trim_start_matches('.');
    if encoder.supports(bare) {
        current
    } else {
        encoder.default_extension().into()
    }
}
fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default()
}
fn same_extension(left: &Path, right: &Path) -> bool {
    extension_of(left).eq_ignore_ascii_case(&extension_of(right))
}
fn unchanged(image: &SourceImage) -> FramePlan {
    FramePlan {
        scaled_width: image.width,
        scaled_height: image.height,
        crop: None,
        output_width: image.width,
        output_height: image.height,
        unchanged: true,
    }
}
fn target_pixels(value: f64, unit: Unit, original: u32, dpi: f64, fit: Fit) -> f64 {
    if is_auto(value) {
        return if fit == Fit::Fit {
            f64::INFINITY
        } else {
            f64::from(original)
        };
    }
    match unit {
        Unit::Inch => value * dpi,
        Unit::Centimeter => value * dpi / 2.54,
        Unit::Percent => value / 100.0 * f64::from(original),
        Unit::Pixel => value,
    }
}
fn axis_scale(target: f64, original: u32) -> f64 {
    if target.is_infinite() {
        f64::INFINITY
    } else {
        target / f64::from(original)
    }
}
fn scaled_dimension(value: f64) -> Result<u32, PlanFault> {
    if !value.is_finite() || value < 0.0 || value > f64::from(i32::MAX) {
        return Err(PlanFault::Dimension);
    }
    Ok(round_to_even(value).max(1) as u32)
}
fn round_to_even(value: f64) -> i64 {
    let floor = value.floor();
    let down = floor as i64;
    let fraction = value - floor;
    let midpoint_up = fraction == 0.5 && down % 2 != 0;
    if fraction > 0.5 || midpoint_up {
        down + 1
    } else {
        down
    }
}
fn is_auto(value: f64) -> bool {
    value == 0.0 || !value.is_finite()
}
fn valid_extent(value: f64) -> bool {
    value.is_finite() && (0.0..=10_000_000.0).contains(&value)
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && !name.contains('|')
        && name.chars().all(|c| !c.is_control())
}
fn quantity(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        let mut text = format!("{value:.8}");
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        text
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn image(width: u32, height: u32) -> SourceImage {
        SourceImage {
            width,
            height,
            dpi_x: 96.0,
            dpi_y: 96.0,
            container: Container::Png,
        }
    }
    fn jpeg(width: u32, height: u32) -> SourceImage {
        SourceImage {
            container: Container::Jpeg,
            ..image(width, height)
        }
    }
    fn photos(name: &str) -> PathBuf {
        PathBuf::from("C:/photos").join(name)
    }
    fn size(fit: Fit, width: f64, height: f64, unit: Unit) -> Size {
        Size {
            name: "custom".into(),
            fit,
            width,
            height,
            unit,
        }
    }
    fn settings(shrink: bool) -> ResizeSettings {
        ResizeSettings {
            shrink_only: shrink,
            ..ResizeSettings::default()
        }
    }
    #[test]
    fn defaults_match_powertoys_presets() {
        let settings = ResizeSettings::default();
        assert_eq!(settings.sizes.len(), 4);
        assert_eq!(settings.sizes[0].width, 854.0);
        assert_eq!(settings.sizes[1].height, 768.0);
        assert_eq!(settings.sizes[2].width, 1920.0);
        assert_eq!(settings.sizes[3].height, 568.0);
        assert!(settings.sizes.iter().all(|size| size.fit == Fit::Fit));
        assert!(!settings.shrink_only);
        assert!(!settings.replace);
        assert!(settings.ignore_orientation);
        assert_eq!(settings.jpeg_quality, 90);
        assert_eq!(settings.filename, "%1 (%2)");
        assert_eq!(settings.fallback, FallbackEncoder::Jpeg);
        assert_eq!(settings.custom.width, 1024.0);
        assert_eq!(settings.custom.height, 640.0);
    }
    #[test]
    fn fit_fill_shrink_and_rounding_follow_powertoys() {
        let source = image(192, 96);
        let grown = size(Fit::Fit, 288.0, 288.0, Unit::Pixel);
        assert!(
            calculate(&source, &grown, &settings(true))
                .unwrap()
                .unchanged
        );
        let square = image(96, 96);
        let fill = calculate(
            &square,
            &size(Fit::Fill, 48.0, 96.0, Unit::Pixel),
            &settings(true),
        )
        .unwrap();
        assert_eq!(fill.output_width, 48);
        assert_eq!(fill.output_height, 96);
        assert_eq!(fill.crop.unwrap().x, 24);
        assert!(
            calculate(
                &square,
                &size(Fit::Fill, 192.0, 192.0, Unit::Pixel),
                &settings(true),
            )
            .unwrap()
            .unchanged
        );
        let small = settings(false).sizes[0].clone();
        let fitted = calculate(&image(1920, 1080), &small, &settings(false)).unwrap();
        assert_eq!((fitted.output_width, fitted.output_height), (853, 480));
        assert_eq!(
            calculate(
                &image(1, 1),
                &size(Fit::Stretch, 1.5, 1.5, Unit::Pixel),
                &settings(false),
            )
            .unwrap()
            .output_width,
            2
        );
        assert_eq!(
            calculate(
                &image(1, 1),
                &size(Fit::Stretch, 2.5, 2.5, Unit::Pixel),
                &settings(false),
            )
            .unwrap()
            .output_width,
            2
        );
    }
    #[test]
    fn units_auto_orientation_and_percent_match_the_converter() {
        let inch = size(Fit::Fit, 1.0, 1.0, Unit::Inch);
        assert_eq!(
            calculate(&image(200, 100), &inch, &settings(false))
                .unwrap()
                .output_width,
            96
        );
        let cm = size(Fit::Stretch, 2.54, 2.54, Unit::Centimeter);
        let converted = calculate(&image(10, 10), &cm, &settings(false)).unwrap();
        assert_eq!((converted.output_width, converted.output_height), (96, 96));
        let percent = size(Fit::Fit, 50.0, 10.0, Unit::Percent);
        let halved = calculate(&image(200, 100), &percent, &settings(false)).unwrap();
        assert_eq!((halved.output_width, halved.output_height), (100, 50));
        let enlarged = calculate(
            &image(10, 10),
            &size(Fit::Fit, 200.0, 200.0, Unit::Percent),
            &settings(true),
        )
        .unwrap();
        assert_eq!(enlarged.output_width, 20);
        let auto = size(Fit::Fit, 0.0, 100.0, Unit::Pixel);
        let kept = calculate(&image(200, 100), &auto, &settings(false)).unwrap();
        assert_eq!((kept.output_width, kept.output_height), (200, 100));
        let portrait = image(100, 200);
        let preset = size(Fit::Fit, 1920.0, 1080.0, Unit::Pixel);
        let swapped = calculate(&portrait, &preset, &settings(false)).unwrap();
        assert_eq!((swapped.output_width, swapped.output_height), (960, 1920));
        let mut raw = settings(false);
        raw.ignore_orientation = false;
        let unswapped = calculate(&portrait, &preset, &raw).unwrap();
        assert_eq!(
            (unswapped.output_width, unswapped.output_height),
            (540, 1080)
        );
    }
    #[test]
    fn names_collisions_and_replace_backups_are_deterministic() {
        let settings = ResizeSettings::default();
        let files = vec![photos("Foto.jpg")];
        let images = vec![Some(jpeg(1920, 1080))];
        let plan = plan_batch(&files, &settings, &images, |_| false);
        let PlanItem::Ready(ready) = &plan[0] else {
            panic!("expected a plan");
        };
        assert_eq!(ready.destination, photos("Foto (Küçük).jpg"));
        let taken = plan_batch(&files, &settings, &images, |path| {
            path.ends_with("Foto (Küçük).jpg")
        });
        let PlanItem::Ready(ready) = &taken[0] else {
            panic!("expected a plan");
        };
        assert_eq!(ready.destination, photos("Foto (Küçük) (1).jpg"));
        let device = vec![photos("CON.jpg")];
        let named = plan_batch(
            &device,
            &ResizeSettings {
                filename: "%1".into(),
                ..settings.clone()
            },
            &[Some(jpeg(1920, 1080))],
            |_| false,
        );
        let PlanItem::Ready(ready) = &named[0] else {
            panic!("expected a plan");
        };
        assert_eq!(ready.destination, photos("CON_.jpg"));
        let pair = vec![photos("A.jpg"), photos("A.jpg"), photos("B.jpg")];
        let copies = plan_batch(
            &pair,
            &ResizeSettings {
                filename: "%1".into(),
                ..settings.clone()
            },
            &[Some(jpeg(20, 20)), Some(jpeg(20, 20)), Some(jpeg(20, 20))],
            |_| false,
        );
        assert_eq!(copies.len(), 2);
        let mut replacing = settings;
        replacing.replace = true;
        let replaced = plan_batch(&files, &replacing, &images, |_| false);
        let PlanItem::Ready(ready) = &replaced[0] else {
            panic!("expected a plan");
        };
        assert_eq!(ready.destination, files[0]);
        assert_eq!(ready.backup.as_ref().unwrap(), &photos("Foto.bak.jpg"));
        let unchanged = plan_batch(
            &[photos("A.png")],
            &ResizeSettings {
                shrink_only: true,
                filename: "%1".into(),
                ..ResizeSettings::default()
            },
            &[Some(image(100, 100))],
            |path| path.file_name().is_some_and(|name| name == "A.png"),
        );
        let PlanItem::Ready(copied) = &unchanged[0] else {
            panic!("expected a plan");
        };
        assert!(copied.frame.unchanged);
        assert!(can_copy_bytes(copied, &ResizeSettings::default()));
    }
}
