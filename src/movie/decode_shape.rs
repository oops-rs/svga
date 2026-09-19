//! Protobuf → [`Shape`]. The geometry is a `type` enum plus one argument
//! message per kind; the arguments matching `type` are used.
use super::{
    Geometry, LineCap, LineJoin, Rgba, Shape, ShapeStyle, Transform,
    decode::{Budget, fold, transform},
};
use crate::error::{Error, Result};

const TYPE_PATH: i32 = 0;
const TYPE_RECT: i32 = 1;
const TYPE_ELLIPSE: i32 = 2;
const TYPE_KEEP: i32 = 3;

/// Fields of a `ShapeEntity` before the geometry is resolved.
#[derive(Default)]
struct Parts<'a> {
    kind: i32,
    path: &'a [u8],
    rect: &'a [u8],
    ellipse: &'a [u8],
    styles: Option<ShapeStyle>,
    transform: Option<Transform>,
}

pub(super) fn shape(payload: &[u8], budget: &Budget) -> Result<Shape> {
    budget.spend()?;
    let parts = fold(payload, Parts::default(), |parts, field| {
        Ok(match field.number {
            1 => Parts {
                kind: field.i32()?,
                ..parts
            },
            2 => Parts {
                path: field.bytes()?,
                ..parts
            },
            3 => Parts {
                rect: field.bytes()?,
                ..parts
            },
            4 => Parts {
                ellipse: field.bytes()?,
                ..parts
            },
            10 => Parts {
                styles: Some(styles(field.bytes()?)?),
                ..parts
            },
            11 => Parts {
                transform: Some(transform(parts.transform, field.bytes()?)?),
                ..parts
            },
            _ => parts,
        })
    })?;
    let geometry = match parts.kind {
        TYPE_PATH => path(parts.path)?,
        TYPE_RECT => rect(parts.rect)?,
        TYPE_ELLIPSE => ellipse(parts.ellipse)?,
        TYPE_KEEP => Geometry::Keep,
        _ => return Err(Error::unsupported("svga_unknown_shape_type")),
    };
    Ok(Shape {
        geometry,
        styles: parts.styles,
        transform: parts.transform,
    })
}

fn path(payload: &[u8]) -> Result<Geometry> {
    let d = fold(payload, "", |d, field| {
        Ok(if field.number == 1 { field.str()? } else { d })
    })?;
    Ok(Geometry::Path { d: d.to_owned() })
}

/// The first `N` float fields of a message, indexed by field number.
fn floats<const N: usize>(payload: &[u8]) -> Result<[f32; N]> {
    fold(payload, [0.0; N], |values, field| {
        let slot = usize::try_from(field.number)
            .ok()
            .and_then(|number| number.checked_sub(1))
            .filter(|slot| *slot < N);
        let Some(slot) = slot else { return Ok(values) };
        let value = field.f32()?;
        let mut updated = values;
        if let Some(target) = updated.get_mut(slot) {
            *target = value;
        }
        Ok(updated)
    })
}

fn rect(payload: &[u8]) -> Result<Geometry> {
    let [x, y, width, height, corner_radius] = floats(payload)?;
    Ok(Geometry::Rect {
        x,
        y,
        width,
        height,
        corner_radius,
    })
}

fn ellipse(payload: &[u8]) -> Result<Geometry> {
    let [x, y, radius_x, radius_y] = floats(payload)?;
    Ok(Geometry::Ellipse {
        x,
        y,
        radius_x,
        radius_y,
    })
}

fn color(payload: &[u8]) -> Result<Rgba> {
    let [r, g, b, a] = floats(payload)?;
    Ok(Rgba { r, g, b, a })
}

fn styles(payload: &[u8]) -> Result<ShapeStyle> {
    fold(payload, ShapeStyle::default(), |style, field| {
        Ok(match field.number {
            1 => ShapeStyle {
                fill: Some(color(field.bytes()?)?),
                ..style
            },
            2 => ShapeStyle {
                stroke: Some(color(field.bytes()?)?),
                ..style
            },
            3 => ShapeStyle {
                stroke_width: field.f32()?,
                ..style
            },
            4 => ShapeStyle {
                line_cap: line_cap(field.i32()?)?,
                ..style
            },
            5 => ShapeStyle {
                line_join: line_join(field.i32()?)?,
                ..style
            },
            6 => ShapeStyle {
                miter_limit: field.f32()?,
                ..style
            },
            7..=9 => {
                let [first, second, third] = style.line_dash;
                let value = field.f32()?;
                let line_dash = match field.number {
                    7 => [value, second, third],
                    8 => [first, value, third],
                    _ => [first, second, value],
                };
                ShapeStyle { line_dash, ..style }
            }
            _ => style,
        })
    })
}

fn line_cap(value: i32) -> Result<LineCap> {
    match value {
        0 => Ok(LineCap::Butt),
        1 => Ok(LineCap::Round),
        2 => Ok(LineCap::Square),
        _ => Err(Error::unsupported("svga_unknown_line_cap")),
    }
}

fn line_join(value: i32) -> Result<LineJoin> {
    match value {
        0 => Ok(LineJoin::Miter),
        1 => Ok(LineJoin::Round),
        2 => Ok(LineJoin::Bevel),
        _ => Err(Error::unsupported("svga_unknown_line_join")),
    }
}
