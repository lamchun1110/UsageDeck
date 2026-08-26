use std::sync::OnceLock;

use fontdue::{Font, FontSettings};
use roxmltree::Document;
use svgtypes::{PathParser, PathSegment};
use tauri::image::Image;
use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Transform};

/// Retina-density bar icon; only the macOS menu bar consumes this size outside tests (Linux
/// renders the StatusNotifier variant, Windows the gauge).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const ICON_SIZE: u32 = 36;
const ICON_POINTS: f32 = 18.0;
pub const MAX_BARS: usize = 4;

/// Linux StatusNotifier panels draw pixmaps verbatim at native pixel size —
/// there is no scale metadata like the Retina 2x representations macOS
/// consumes — so Linux tray artifacts are authored at the ~24px panel icon
/// height. Compiled everywhere so the density stays unit-testable off-Linux.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const STATUS_NOTIFIER_SIZE: u32 = 24;

/// Glyph color for tray strips. macOS template images are recolored by the
/// system, but StatusNotifier panels draw pixmaps verbatim — so Linux needs
/// the tone to match the panel rather than relying on the desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphTone {
    Dark,
    // Only Linux consumes Light today; macOS template images are recolored by
    // the system, so there Dark is always passed.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Light,
}

impl GlyphTone {
    fn rgb(self) -> [u8; 3] {
        match self {
            GlyphTone::Dark => [26, 26, 31],
            GlyphTone::Light => [242, 242, 245],
        }
    }
}

/// Text strip geometry in device pixels. One set per platform density: macOS
/// menu bar items are 18pt with the strip shipped as a 2x Retina template,
/// while StatusNotifier panels need glyphs authored at the panel icon height.
#[derive(Debug, Clone, Copy)]
struct StripMetrics {
    height: u32,
    outer_padding: f32,
    group_gap: f32,
    icon_text_gap: f32,
    provider_icon_size: f32,
    provider_icon_inset: f32,
    single_value_size: f32,
    stacked_value_size: f32,
    stacked_baselines: [f32; 2],
}

impl StripMetrics {
    /// macOS menu bar template strip: 18pt at 2x Retina density.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    const RETINA: Self = Self {
        height: 36,
        outer_padding: 2.0,
        group_gap: 22.0,
        icon_text_gap: 8.0,
        provider_icon_size: 32.0,
        provider_icon_inset: 1.0,
        single_value_size: 23.0,
        stacked_value_size: 17.0,
        stacked_baselines: [15.0, 32.0],
    };

    /// Linux StatusNotifier strip: 24px tall, proportions matched to RETINA.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    const STATUS_NOTIFIER: Self = Self {
        height: STATUS_NOTIFIER_SIZE,
        outer_padding: 1.0,
        group_gap: 15.0,
        icon_text_gap: 5.0,
        provider_icon_size: 20.0,
        provider_icon_inset: 1.0,
        single_value_size: 15.0,
        stacked_value_size: 11.0,
        stacked_baselines: [10.0, 21.0],
    };
}

const FONT_SOURCE: &[u8] = include_bytes!("../assets/fonts/Poppins-SemiBold.ttf");
const BRAND_SOURCE: &str = include_str!("../../assets/usagedeck-tray.svg");

const CLAUDE_ICON: &str = include_str!("../../src/assets/provider-icons/claude.svg");
const COMMANDCODE_ICON: &str = include_str!("../../src/assets/provider-icons/commandcode.svg");
const CODEX_ICON: &str = include_str!("../../src/assets/provider-icons/codex.svg");
const COPILOT_ICON: &str = include_str!("../../src/assets/provider-icons/copilot.svg");
const CURSOR_ICON: &str = include_str!("../../src/assets/provider-icons/cursor.svg");
const DEVIN_ICON: &str = include_str!("../../src/assets/provider-icons/devin.svg");
const ANTIGRAVITY_ICON: &str = include_str!("../../src/assets/provider-icons/antigravity.svg");
const GROK_ICON: &str = include_str!("../../src/assets/provider-icons/grok.svg");
const OPENCODE_ICON: &str = include_str!("../../src/assets/provider-icons/opencode.svg");
const OPENROUTER_ICON: &str = include_str!("../../src/assets/provider-icons/openrouter.svg");
const ZAI_ICON: &str = include_str!("../../src/assets/provider-icons/zai.svg");
const KIMI_ICON: &str = include_str!("../../src/assets/provider-icons/kimi.svg");
const MINIMAX_ICON: &str = include_str!("../../src/assets/provider-icons/minimax.svg");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextGroup {
    pub provider_id: String,
    pub values: Vec<String>,
}

/// Retina-density strip consumed by the macOS menu bar; Linux renders the StatusNotifier
/// variant below and Windows the gauge icon.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn text_icon(groups: &[TextGroup], tone: GlyphTone) -> Option<Image<'static>> {
    let strip = render_text_strip(groups, tone, StripMetrics::RETINA)?;
    Some(Image::new_owned(
        strip.rgba,
        strip.width,
        StripMetrics::RETINA.height,
    ))
}

/// Provider strip at StatusNotifier panel density; see `STATUS_NOTIFIER_SIZE`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn status_notifier_text_icon(groups: &[TextGroup], tone: GlyphTone) -> Option<Image<'static>> {
    let strip = render_text_strip(groups, tone, StripMetrics::STATUS_NOTIFIER)?;
    Some(Image::new_owned(
        strip.rgba,
        strip.width,
        StripMetrics::STATUS_NOTIFIER.height,
    ))
}

/// Retina-density bars consumed by the macOS menu bar; see `text_icon` for the platform split.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn bar_icon(fractions: &[f64], tone: GlyphTone) -> Image<'static> {
    Image::new_owned(
        render_bar_rgba(fractions, tone, ICON_SIZE),
        ICON_SIZE,
        ICON_SIZE,
    )
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn status_notifier_bar_icon(fractions: &[f64], tone: GlyphTone) -> Image<'static> {
    Image::new_owned(
        render_bar_rgba(fractions, tone, STATUS_NOTIFIER_SIZE),
        STATUS_NOTIFIER_SIZE,
        STATUS_NOTIFIER_SIZE,
    )
}

/// Fallback mark for Linux trays: the bundled brand glyph, tone-colored and
/// sized for the panel — the 32px PNG the macOS path uses would render a
/// third larger than neighbouring status icons.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn status_notifier_mark_icon(tone: GlyphTone) -> Image<'static> {
    let mut pixmap = Pixmap::new(STATUS_NOTIFIER_SIZE, STATUS_NOTIFIER_SIZE)
        .expect("status notifier mark dimensions are valid");
    let path = brand_mark_path();
    let [red, green, blue] = tone.rgb();
    let mut paint = Paint::default();
    paint.set_color_rgba8(red, green, blue, 255);
    paint.anti_alias = true;
    let bounds = path.bounds();
    let target = STATUS_NOTIFIER_SIZE as f32 - 4.0;
    let scale = (target / bounds.width()).min(target / bounds.height());
    let tx = (STATUS_NOTIFIER_SIZE as f32 - bounds.width() * scale) / 2.0 - bounds.left() * scale;
    let ty = (STATUS_NOTIFIER_SIZE as f32 - bounds.height() * scale) / 2.0 - bounds.top() * scale;
    pixmap.fill_path(
        path,
        &paint,
        FillRule::Winding,
        Transform::from_row(scale, 0.0, 0.0, scale, tx, ty),
        None,
    );
    Image::new_owned(
        pixmap.take_demultiplied(),
        STATUS_NOTIFIER_SIZE,
        STATUS_NOTIFIER_SIZE,
    )
}

fn brand_mark_path() -> &'static Path {
    static BRAND: OnceLock<Path> = OnceLock::new();
    BRAND.get_or_init(|| {
        let mark = Document::parse(BRAND_SOURCE)
            .map_err(|error| error.to_string())
            .and_then(|document| {
                let data = document
                    .descendants()
                    .find(|node| node.is_element() && node.attribute("id") == Some("brand-mark"))
                    .and_then(|node| node.attribute("d"))
                    .ok_or_else(|| "missing #brand-mark".to_owned())?;
                parse_path_data(&[data])
            });
        mark.unwrap_or_else(|error| panic!("invalid bundled tray SVG: {error}"))
    })
}

struct RenderedStrip {
    rgba: Vec<u8>,
    width: u32,
}

#[derive(Debug, Clone)]
struct GroupLayout<'a> {
    group: &'a TextGroup,
    text_width: f32,
    width: f32,
}

fn render_text_strip(
    groups: &[TextGroup],
    tone: GlyphTone,
    metrics: StripMetrics,
) -> Option<RenderedStrip> {
    let groups = groups
        .iter()
        .filter(|group| !group.values.is_empty())
        .map(|group| {
            let text_width = group
                .values
                .iter()
                .take(2)
                .map(|value| {
                    measure_text(
                        value,
                        if group.values.len() == 1 {
                            metrics.single_value_size
                        } else {
                            metrics.stacked_value_size
                        },
                    )
                })
                .fold(0.0_f32, f32::max)
                .ceil();
            GroupLayout {
                group,
                text_width,
                width: metrics.provider_icon_size + metrics.icon_text_gap + text_width,
            }
        })
        .collect::<Vec<_>>();
    if groups.is_empty() {
        return None;
    }

    let content_width = groups.iter().map(|group| group.width).sum::<f32>()
        + metrics.group_gap * groups.len().saturating_sub(1) as f32;
    let width = (content_width + metrics.outer_padding * 2.0)
        .ceil()
        .max(1.0) as u32;
    let mut pixmap =
        Pixmap::new(width, metrics.height).expect("menu bar strip dimensions are valid");
    let mut x = metrics.outer_padding;

    for layout in groups {
        draw_provider_icon(&mut pixmap, &layout.group.provider_id, x, tone, metrics);
        let text_x = x + metrics.provider_icon_size + metrics.icon_text_gap;
        if layout.group.values.len() == 1 {
            let value = &layout.group.values[0];
            let baseline =
                centered_baseline(value, metrics.single_value_size, metrics.height as f32);
            draw_text(
                &mut pixmap,
                value,
                metrics.single_value_size,
                text_x,
                baseline,
                tone,
            );
        } else {
            for (value, baseline) in layout
                .group
                .values
                .iter()
                .take(2)
                .zip(metrics.stacked_baselines)
            {
                let value_width = measure_text(value, metrics.stacked_value_size);
                draw_text(
                    &mut pixmap,
                    value,
                    metrics.stacked_value_size,
                    text_x + layout.text_width - value_width,
                    baseline,
                    tone,
                );
            }
        }
        x += layout.width + metrics.group_gap;
    }

    Some(RenderedStrip {
        rgba: pixmap.take_demultiplied(),
        width,
    })
}

fn bundled_font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        Font::from_bytes(FONT_SOURCE, FontSettings::default())
            .expect("bundled menu bar font must be valid")
    })
}

fn centered_baseline(text: &str, size: f32, height: f32) -> f32 {
    let font = bundled_font();
    let mut top = f32::MAX;
    let mut bottom = f32::MIN;
    for character in text.chars() {
        let metrics = font.metrics(character, size);
        top = top.min(-(metrics.height as f32 + metrics.ymin as f32));
        bottom = bottom.max(-(metrics.ymin as f32));
    }
    if top == f32::MAX {
        return height / 2.0;
    }
    (height - (bottom - top)) / 2.0 - top
}

fn measure_text(text: &str, size: f32) -> f32 {
    let font = bundled_font();
    let mut width = 0.0;
    let mut previous = None;
    for character in text.chars() {
        if let Some(previous) = previous {
            width += font
                .horizontal_kern(previous, character, size)
                .unwrap_or(0.0);
        }
        width += font.metrics(character, size).advance_width;
        previous = Some(character);
    }
    width.max(0.0)
}

fn draw_text(pixmap: &mut Pixmap, text: &str, size: f32, x: f32, baseline: f32, tone: GlyphTone) {
    let font = bundled_font();
    let mut pen_x = x;
    let mut previous = None;
    for character in text.chars() {
        if let Some(previous) = previous {
            pen_x += font
                .horizontal_kern(previous, character, size)
                .unwrap_or(0.0);
        }
        let (metrics, bitmap) = font.rasterize(character, size);
        let glyph_x = (pen_x + metrics.xmin as f32).round() as i32;
        let glyph_y = (baseline - metrics.height as f32 - metrics.ymin as f32).round() as i32;
        blend_alpha_mask(
            pixmap,
            &bitmap,
            metrics.width,
            metrics.height,
            glyph_x,
            glyph_y,
            tone.rgb(),
        );
        pen_x += metrics.advance_width;
        previous = Some(character);
    }
}

fn blend_alpha_mask(
    pixmap: &mut Pixmap,
    mask: &[u8],
    mask_width: usize,
    mask_height: usize,
    target_x: i32,
    target_y: i32,
    [red, green, blue]: [u8; 3],
) {
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let pixels = pixmap.data_mut();
    for source_y in 0..mask_height {
        let y = target_y + source_y as i32;
        if !(0..height).contains(&y) {
            continue;
        }
        for source_x in 0..mask_width {
            let x = target_x + source_x as i32;
            if !(0..width).contains(&x) {
                continue;
            }
            let alpha = mask[source_y * mask_width + source_x];
            let target = ((y * width + x) * 4) as usize;
            if alpha > pixels[target + 3] {
                pixels[target] = red;
                pixels[target + 1] = green;
                pixels[target + 2] = blue;
            }
            pixels[target + 3] = pixels[target + 3].max(alpha);
        }
    }
}

fn draw_provider_icon(
    pixmap: &mut Pixmap,
    provider_id: &str,
    x: f32,
    tone: GlyphTone,
    metrics: StripMetrics,
) {
    let [red, green, blue] = tone.rgb();
    let mut paint = Paint::default();
    paint.set_color_rgba8(red, green, blue, 255);
    paint.anti_alias = true;
    let icon_top = (metrics.height as f32 - metrics.provider_icon_size) / 2.0;

    if let Some(path) = provider_path(provider_id) {
        let bounds = path.bounds();
        let target = metrics.provider_icon_size - metrics.provider_icon_inset * 2.0;
        let scale = (target / bounds.width()).min(target / bounds.height());
        let tx = x + metrics.provider_icon_inset + (target - bounds.width() * scale) / 2.0
            - bounds.left() * scale;
        let ty = icon_top + metrics.provider_icon_inset + (target - bounds.height() * scale) / 2.0
            - bounds.top() * scale;
        pixmap.fill_path(
            path,
            &paint,
            FillRule::Winding,
            Transform::from_row(scale, 0.0, 0.0, scale, tx, ty),
            None,
        );
    } else {
        let mut fallback = PathBuilder::new();
        fallback.push_circle(
            x + metrics.provider_icon_size / 2.0,
            icon_top + metrics.provider_icon_size / 2.0,
            (metrics.provider_icon_size - 2.0) / 2.0,
        );
        if let Some(path) = fallback.finish() {
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
}

fn provider_path(provider_id: &str) -> Option<&'static Path> {
    fn parsed(source: &'static str, slot: &'static OnceLock<Path>) -> &'static Path {
        slot.get_or_init(|| {
            parse_svg_path(source)
                .unwrap_or_else(|error| panic!("invalid bundled provider SVG: {error}"))
        })
    }

    static CLAUDE: OnceLock<Path> = OnceLock::new();
    static COMMANDCODE: OnceLock<Path> = OnceLock::new();
    static CODEX: OnceLock<Path> = OnceLock::new();
    static COPILOT: OnceLock<Path> = OnceLock::new();
    static CURSOR: OnceLock<Path> = OnceLock::new();
    static DEVIN: OnceLock<Path> = OnceLock::new();
    static ANTIGRAVITY: OnceLock<Path> = OnceLock::new();
    static GROK: OnceLock<Path> = OnceLock::new();
    static OPENCODE: OnceLock<Path> = OnceLock::new();
    static OPENROUTER: OnceLock<Path> = OnceLock::new();
    static ZAI: OnceLock<Path> = OnceLock::new();
    static KIMI: OnceLock<Path> = OnceLock::new();
    static MINIMAX: OnceLock<Path> = OnceLock::new();
    match crate::providers::provider_family(provider_id) {
        "claude" => Some(parsed(CLAUDE_ICON, &CLAUDE)),
        "commandcode" => Some(parsed(COMMANDCODE_ICON, &COMMANDCODE)),
        "codex" => Some(parsed(CODEX_ICON, &CODEX)),
        "copilot" => Some(parsed(COPILOT_ICON, &COPILOT)),
        "cursor" => Some(parsed(CURSOR_ICON, &CURSOR)),
        "devin" => Some(parsed(DEVIN_ICON, &DEVIN)),
        "antigravity" => Some(parsed(ANTIGRAVITY_ICON, &ANTIGRAVITY)),
        "grok" => Some(parsed(GROK_ICON, &GROK)),
        "opencode" => Some(parsed(OPENCODE_ICON, &OPENCODE)),
        "openrouter" => Some(parsed(OPENROUTER_ICON, &OPENROUTER)),
        "zai" => Some(parsed(ZAI_ICON, &ZAI)),
        "kimi" => Some(parsed(KIMI_ICON, &KIMI)),
        "minimax" => Some(parsed(MINIMAX_ICON, &MINIMAX)),
        _ => None,
    }
}

fn parse_svg_path(source: &str) -> Result<Path, String> {
    let document = Document::parse(source).map_err(|error| error.to_string())?;
    let path_data = document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "path")
        .filter_map(|node| node.attribute("d"))
        .collect::<Vec<_>>();
    if path_data.is_empty() {
        return Err("missing path data".to_owned());
    }
    parse_path_data(&path_data)
}

/// Combines one or more SVG path `d` fragments into a single path.
fn parse_path_data(path_data: &[&str]) -> Result<Path, String> {
    let mut builder = PathBuilder::new();
    for &data in path_data {
        let mut current = None;
        let mut subpath_start = None;
        let mut previous_cubic_control = None;
        for segment in PathParser::from(data) {
            match segment.map_err(|error| error.to_string())? {
                PathSegment::MoveTo { abs, x, y } => {
                    let origin = if abs {
                        (0.0, 0.0)
                    } else {
                        current.unwrap_or((0.0, 0.0))
                    };
                    let point = (origin.0 + x as f32, origin.1 + y as f32);
                    builder.move_to(point.0, point.1);
                    current = Some(point);
                    subpath_start = Some(point);
                    previous_cubic_control = None;
                }
                PathSegment::LineTo { abs, x, y } => {
                    let origin = if abs {
                        (0.0, 0.0)
                    } else {
                        current.ok_or_else(|| "relative line has no current point".to_owned())?
                    };
                    let point = (origin.0 + x as f32, origin.1 + y as f32);
                    builder.line_to(point.0, point.1);
                    current = Some(point);
                    previous_cubic_control = None;
                }
                PathSegment::HorizontalLineTo { abs, x } => {
                    let (current_x, current_y) =
                        current.ok_or_else(|| "horizontal line has no current point".to_owned())?;
                    let point = (if abs { x as f32 } else { current_x + x as f32 }, current_y);
                    builder.line_to(point.0, point.1);
                    current = Some(point);
                    previous_cubic_control = None;
                }
                PathSegment::VerticalLineTo { abs, y } => {
                    let (current_x, current_y) =
                        current.ok_or_else(|| "vertical line has no current point".to_owned())?;
                    let point = (current_x, if abs { y as f32 } else { current_y + y as f32 });
                    builder.line_to(point.0, point.1);
                    current = Some(point);
                    previous_cubic_control = None;
                }
                PathSegment::CurveTo {
                    abs,
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => {
                    let origin = if abs {
                        (0.0, 0.0)
                    } else {
                        current.ok_or_else(|| "relative curve has no current point".to_owned())?
                    };
                    let end = (origin.0 + x as f32, origin.1 + y as f32);
                    builder.cubic_to(
                        origin.0 + x1 as f32,
                        origin.1 + y1 as f32,
                        origin.0 + x2 as f32,
                        origin.1 + y2 as f32,
                        end.0,
                        end.1,
                    );
                    current = Some(end);
                    previous_cubic_control = Some((origin.0 + x2 as f32, origin.1 + y2 as f32));
                }
                PathSegment::SmoothCurveTo { abs, x2, y2, x, y } => {
                    let origin =
                        current.ok_or_else(|| "smooth curve has no current point".to_owned())?;
                    let first = previous_cubic_control
                        .map(|control| (origin.0 * 2.0 - control.0, origin.1 * 2.0 - control.1))
                        .unwrap_or(origin);
                    let coordinate_origin = if abs { (0.0, 0.0) } else { origin };
                    let second = (
                        coordinate_origin.0 + x2 as f32,
                        coordinate_origin.1 + y2 as f32,
                    );
                    let end = (
                        coordinate_origin.0 + x as f32,
                        coordinate_origin.1 + y as f32,
                    );
                    builder.cubic_to(first.0, first.1, second.0, second.1, end.0, end.1);
                    current = Some(end);
                    previous_cubic_control = Some(second);
                }
                PathSegment::ClosePath { .. } => {
                    builder.close();
                    current = subpath_start;
                    previous_cubic_control = None;
                }
                _ => return Err("only M, L, H, V, C, S and Z path commands are supported".into()),
            }
        }
    }
    builder.finish().ok_or_else(|| "path is empty".into())
}

fn render_bar_rgba(fractions: &[f64], tone: GlyphTone, icon_size: u32) -> Vec<u8> {
    let scale = icon_size as f32 / ICON_POINTS;
    let size = ICON_POINTS;
    let mut pixmap = Pixmap::new(icon_size, icon_size).expect("menu bar icon dimensions are valid");
    let count = fractions.len().min(MAX_BARS);
    if count == 0 {
        return pixmap.take_demultiplied();
    }

    let padding = (size * 0.08).round().max(1.0);
    let gap = (size * 0.03).round().max(1.0);
    let track_x = padding;
    let track_width = size - 2.0 * padding;
    let layout_count = count.max(2) as f32;
    let track_height = ((size - 2.0 * padding - (layout_count - 1.0) * gap) / layout_count)
        .floor()
        .max(1.0);
    let radius = (track_height / 3.0).floor().max(1.0);
    let total_height = count as f32 * track_height + count.saturating_sub(1) as f32 * gap;
    let y_offset = padding + ((size - 2.0 * padding - total_height) / 2.0).floor();

    for (index, fraction) in fractions.iter().take(MAX_BARS).enumerate() {
        let y = y_offset + index as f32 * (track_height + gap) + 1.0;
        fill_rounded_bar(
            &mut pixmap,
            track_x * scale,
            y * scale,
            track_width * scale,
            track_height * scale,
            radius * scale,
            radius * scale,
            tone,
            41,
        );

        let fill = bar_fill(track_width, *fraction);
        if fill.fill_width > 0.0 {
            let trailing = if fill.fill_width >= track_width {
                radius
            } else {
                (radius * 0.35).floor().max(0.0)
            };
            fill_rounded_bar(
                &mut pixmap,
                track_x * scale,
                y * scale,
                fill.fill_width * scale,
                track_height * scale,
                radius * scale,
                trailing * scale,
                tone,
                255,
            );
        }
        if let Some(divider_x) = fill.divider_x {
            fill_rounded_bar(
                &mut pixmap,
                (track_x + divider_x) * scale,
                y * scale,
                fill.remainder_width * scale,
                track_height * scale,
                (radius * 0.2).floor().max(0.0) * scale,
                radius * scale,
                tone,
                61,
            );
        }
    }
    pixmap.take_demultiplied()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BarFill {
    fill_width: f32,
    remainder_width: f32,
    divider_x: Option<f32>,
}

fn visual_bar_fraction(fraction: f64) -> f64 {
    if !fraction.is_finite() {
        return 0.0;
    }
    let clamped = fraction.clamp(0.0, 1.0);
    if clamped > 0.7 && clamped < 1.0 {
        let remainder = 1.0 - clamped;
        let quantized = ((remainder / 0.15).ceil() * 0.15).min(1.0);
        1.0 - quantized
    } else {
        clamped
    }
}

fn bar_fill(track_width: f32, fraction: f64) -> BarFill {
    if !fraction.is_finite() || fraction <= 0.0 {
        return BarFill {
            fill_width: 0.0,
            remainder_width: 0.0,
            divider_x: None,
        };
    }
    let visual = visual_bar_fraction(fraction);
    if visual >= 1.0 {
        return BarFill {
            fill_width: track_width,
            remainder_width: 0.0,
            divider_x: None,
        };
    }
    let min_visible = 4.0_f32.max((track_width * 0.2).round());
    let max_fill_width = 1.0_f32.max(track_width - min_visible);
    let fill_width = 1.0_f32.max(max_fill_width.min((track_width * visual as f32).round()));
    let true_remainder = track_width - fill_width;
    let remainder_width = (track_width - 1.0).min(true_remainder.max(min_visible));
    BarFill {
        fill_width,
        remainder_width,
        divider_x: Some(track_width - remainder_width),
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_rounded_bar(
    pixmap: &mut Pixmap,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    leading_radius: f32,
    trailing_radius: f32,
    tone: GlyphTone,
    alpha: u8,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let leading = leading_radius.min(height / 2.0).min(width / 2.0);
    let trailing = trailing_radius.min(height / 2.0).min(width / 2.0);
    let mut builder = PathBuilder::new();
    builder.move_to(x + leading, y);
    builder.line_to(x + width - trailing, y);
    builder.quad_to(x + width, y, x + width, y + trailing);
    builder.line_to(x + width, y + height - trailing);
    builder.quad_to(x + width, y + height, x + width - trailing, y + height);
    builder.line_to(x + leading, y + height);
    builder.quad_to(x, y + height, x, y + height - leading);
    builder.line_to(x, y + leading);
    builder.quad_to(x, y, x + leading, y);
    builder.close();
    let Some(path): Option<Path> = builder.finish() else {
        return;
    };
    let [red, green, blue] = tone.rgb();
    let mut paint = Paint::default();
    paint.set_color_rgba8(red, green, blue, alpha);
    paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::{
        bar_fill, bar_icon, parse_svg_path, provider_path, render_bar_rgba, render_text_strip,
        status_notifier_bar_icon, status_notifier_mark_icon, status_notifier_text_icon, text_icon,
        visual_bar_fraction, GlyphTone, StripMetrics, TextGroup, ICON_SIZE, MAX_BARS,
        STATUS_NOTIFIER_SIZE,
    };

    fn text_group(provider_id: &str, values: &[&str]) -> TextGroup {
        TextGroup {
            provider_id: provider_id.into(),
            values: values.iter().map(|value| (*value).into()).collect(),
        }
    }

    #[test]
    fn bundled_provider_marks_and_font_render_into_a_retina_text_strip() {
        for provider in [
            "claude",
            "commandcode",
            "codex",
            "copilot",
            "cursor",
            "devin",
            "antigravity",
            "grok",
            "opencode",
            "openrouter",
            "zai",
            "kimi",
            "minimax",
        ] {
            let path = provider_path(provider).expect("known provider mark should exist");
            assert!(path.bounds().width() > 0.0);
            assert!(path.bounds().height() > 0.0);
        }

        let strip = render_text_strip(
            &[
                text_group("claude", &["100%", "36%"]),
                text_group("codex", &["100%", "89%"]),
                text_group("cursor", &["93%", "0%"]),
            ],
            GlyphTone::Dark,
            StripMetrics::RETINA,
        )
        .expect("provider values should produce a strip");
        assert_eq!(
            strip.rgba.len(),
            (strip.width * StripMetrics::RETINA.height * 4) as usize
        );
        assert!(strip.width > StripMetrics::RETINA.height * 3);
        assert!(strip
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] == 255));
        let icon = text_icon(&[text_group("codex", &["75%", "40%"])], GlyphTone::Light)
            .expect("public text renderer should return an image");
        assert_eq!(icon.height(), StripMetrics::RETINA.height);
        assert!(icon.width() > StripMetrics::RETINA.height);
    }

    #[test]
    fn provider_mark_parser_combines_paths_and_resolves_relative_commands() {
        let path = parse_svg_path(
            r#"<svg><path d="M1 1l2 0v2h-2z"/><path d="M10 10c1 0 2 1 3 2s2 1 3 2"/></svg>"#,
        )
        .expect("valid provider paths should be combined");

        assert!(path.bounds().width() >= 15.0);
        assert!(path.bounds().height() >= 13.0);
    }

    #[test]
    fn text_strip_uses_natural_width_and_ignores_empty_groups() {
        assert!(render_text_strip(&[], GlyphTone::Dark, StripMetrics::RETINA).is_none());
        assert!(render_text_strip(
            &[text_group("codex", &[])],
            GlyphTone::Dark,
            StripMetrics::RETINA
        )
        .is_none());

        let one = render_text_strip(
            &[text_group("codex", &["75%"])],
            GlyphTone::Dark,
            StripMetrics::RETINA,
        )
        .expect("one group should render");
        let two = render_text_strip(
            &[
                text_group("codex", &["75%"]),
                text_group("claude", &["80%", "40%"]),
            ],
            GlyphTone::Dark,
            StripMetrics::RETINA,
        )
        .expect("two groups should render");
        assert_eq!(
            one.rgba.len(),
            (one.width * StripMetrics::RETINA.height * 4) as usize
        );
        assert!(two.width > one.width);
    }

    #[test]
    fn unknown_providers_receive_a_visible_neutral_fallback_mark() {
        assert!(provider_path("future-provider").is_none());
        let strip = render_text_strip(
            &[text_group("future-provider", &["42%"])],
            GlyphTone::Dark,
            StripMetrics::RETINA,
        )
        .expect("unknown provider should still render");
        assert!(strip
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] == 255));
    }

    #[test]
    fn account_cards_reuse_their_provider_family_mark() {
        assert!(std::ptr::eq(
            provider_path("claude").unwrap(),
            provider_path("claude@1234abcd").unwrap()
        ));
    }

    #[test]
    fn renderer_preserves_empty_zero_and_full_states() {
        let alpha_pixels = |rgba: &[u8]| {
            rgba.as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] > 0)
                .count()
        };
        let empty = render_bar_rgba(&[], GlyphTone::Dark, ICON_SIZE);
        let zero = render_bar_rgba(&[0.0], GlyphTone::Dark, ICON_SIZE);
        let half = render_bar_rgba(&[0.5], GlyphTone::Dark, ICON_SIZE);
        let full = render_bar_rgba(&[1.0], GlyphTone::Dark, ICON_SIZE);

        assert_eq!(empty.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        assert_eq!(alpha_pixels(&empty), 0);
        assert!(alpha_pixels(&zero) > 0);
        assert!(zero.as_chunks::<4>().0.iter().all(|pixel| pixel[3] < 255));

        let visible = zero
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, pixel)| pixel[3] > 0)
            .map(|(index, _)| (index % ICON_SIZE as usize, index / ICON_SIZE as usize))
            .collect::<Vec<_>>();
        let min_x = visible.iter().map(|point| point.0).min().unwrap();
        let max_x = visible.iter().map(|point| point.0).max().unwrap();
        let min_y = visible.iter().map(|point| point.1).min().unwrap();
        let max_y = visible.iter().map(|point| point.1).max().unwrap();
        assert!(max_x - min_x > max_y - min_y);
        assert!(half.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 255));
        assert!(full.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 255));
    }

    #[test]
    fn renderer_sanitizes_values_and_caps_the_visible_metric_count() {
        assert_eq!(
            render_bar_rgba(&[f64::NAN, -1.0, 2.0], GlyphTone::Dark, ICON_SIZE),
            render_bar_rgba(&[0.0, 0.0, 1.0], GlyphTone::Dark, ICON_SIZE)
        );
        let fractions = [0.1, 0.3, 0.6, 1.0, 0.8];
        assert_eq!(
            render_bar_rgba(&fractions, GlyphTone::Dark, ICON_SIZE),
            render_bar_rgba(&fractions[..MAX_BARS], GlyphTone::Dark, ICON_SIZE)
        );
    }

    #[test]
    fn near_full_bars_keep_a_visible_remainder() {
        assert_eq!(visual_bar_fraction(0.0), 0.0);
        assert_eq!(visual_bar_fraction(0.5), 0.5);
        assert!((visual_bar_fraction(0.97) - 0.85).abs() < 0.0001);
        assert_eq!(visual_bar_fraction(1.0), 1.0);

        let near_full = bar_fill(16.0, 0.97);
        assert_eq!(near_full.fill_width, 12.0);
        assert_eq!(near_full.remainder_width, 4.0);
        assert_eq!(near_full.divider_x, Some(12.0));

        let full = bar_fill(16.0, 1.0);
        assert_eq!(full.fill_width, 16.0);
        assert_eq!(full.remainder_width, 0.0);
        assert_eq!(full.divider_x, None);
    }

    #[test]
    fn icon_uses_a_retina_density_square() {
        let icon = bar_icon(&[0.5], GlyphTone::Dark);
        assert_eq!((icon.width(), icon.height()), (36, 36));
    }

    #[test]
    fn glyph_tone_controls_the_painted_color_for_status_notifier_panels() {
        let bright_pixels = |rgba: &[u8]| {
            rgba.as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] == 255 && pixel[0] > 200)
                .count()
        };
        let dark = render_text_strip(
            &[text_group("codex", &["75%"])],
            GlyphTone::Dark,
            StripMetrics::RETINA,
        )
        .expect("dark strip should render");
        let light = render_text_strip(
            &[text_group("codex", &["75%"])],
            GlyphTone::Light,
            StripMetrics::RETINA,
        )
        .expect("light strip should render");
        assert_eq!(bright_pixels(&dark.rgba), 0);
        assert!(bright_pixels(&light.rgba) > 0);
    }

    #[test]
    fn status_notifier_icons_render_at_panel_density() {
        let strip = status_notifier_text_icon(&[text_group("zai", &["96%"])], GlyphTone::Light)
            .expect("status notifier strip should render");
        assert_eq!(strip.height(), STATUS_NOTIFIER_SIZE);
        assert!(strip.width() > STATUS_NOTIFIER_SIZE);
        assert!(strip
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 0));

        let bars = status_notifier_bar_icon(&[0.5], GlyphTone::Light);
        assert_eq!(
            (bars.width(), bars.height()),
            (STATUS_NOTIFIER_SIZE, STATUS_NOTIFIER_SIZE)
        );

        let mark = status_notifier_mark_icon(GlyphTone::Light);
        assert_eq!(
            (mark.width(), mark.height()),
            (STATUS_NOTIFIER_SIZE, STATUS_NOTIFIER_SIZE)
        );
        assert!(mark
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn status_notifier_strip_is_compacter_than_the_retina_strip() {
        let groups = [text_group("codex", &["75%", "40%"])];
        let retina = text_icon(&groups, GlyphTone::Dark).expect("retina strip should render");
        let panel =
            status_notifier_text_icon(&groups, GlyphTone::Dark).expect("panel strip should render");
        assert!(panel.height() < retina.height());
        assert!(panel.width() < retina.width());
    }
}
