//! Shared SVG path parsing for the tray and menu-bar renderers.
//!
//! One parser, one accepted grammar: absolute and relative M, L, H, V, C, S
//! and Z across multiple `d` fragments. A bundled asset can no longer parse
//! in one renderer and fail in the other.

use svgtypes::{PathParser, PathSegment};
use tiny_skia::{Path, PathBuilder};

pub(crate) fn parse_path_data(path_data: &[&str]) -> Result<Path, String> {
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
