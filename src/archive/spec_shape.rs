//! SVGA 1.x shape JSON → [`Shape`].
use super::spec::{Budget, items, number, text, transform};
use crate::{
    error::{Error, Result},
    movie::{Geometry, LineCap, LineJoin, Rgba, Shape, ShapeStyle},
};
use serde_json::Value;

pub(super) fn shape(value: &Value, budget: &Budget) -> Result<Shape> {
    budget.spend()?;
    let arguments = value.get("args").unwrap_or(&Value::Null);
    let geometry = match text(value, "type").as_str() {
        "" | "shape" => Geometry::Path {
            d: text(arguments, "d"),
        },
        "rect" => Geometry::Rect {
            x: number(arguments, "x"),
            y: number(arguments, "y"),
            width: number(arguments, "width"),
            height: number(arguments, "height"),
            corner_radius: number(arguments, "cornerRadius"),
        },
        "ellipse" => Geometry::Ellipse {
            x: number(arguments, "x"),
            y: number(arguments, "y"),
            radius_x: number(arguments, "radiusX"),
            radius_y: number(arguments, "radiusY"),
        },
        "keep" => Geometry::Keep,
        _ => return Err(Error::unsupported("svga_unknown_shape_type")),
    };
    Ok(Shape {
        geometry,
        styles: value.get("styles").filter(|v| v.is_object()).map(styles),
        transform: transform(value),
    })
}

fn component(values: &[Value], index: usize) -> f32 {
    values.get(index).and_then(Value::as_f64).unwrap_or(0.0) as f32
}

fn color(owner: &Value, member: &str) -> Option<Rgba> {
    let values = owner.get(member)?.as_array()?;
    Some(Rgba {
        r: component(values, 0),
        g: component(values, 1),
        b: component(values, 2),
        a: component(values, 3),
    })
}

fn styles(value: &Value) -> ShapeStyle {
    let dash = items(value, "lineDash");
    ShapeStyle {
        fill: color(value, "fill"),
        stroke: color(value, "stroke"),
        stroke_width: number(value, "strokeWidth"),
        line_cap: match text(value, "lineCap").as_str() {
            "round" => LineCap::Round,
            "square" => LineCap::Square,
            _ => LineCap::Butt,
        },
        line_join: match text(value, "lineJoin").as_str() {
            "round" => LineJoin::Round,
            "bevel" => LineJoin::Bevel,
            _ => LineJoin::Miter,
        },
        miter_limit: number(value, "miterLimit"),
        line_dash: [component(dash, 0), component(dash, 1), component(dash, 2)],
    }
}
