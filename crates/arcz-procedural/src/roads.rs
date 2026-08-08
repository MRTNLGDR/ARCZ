use arcz_validation::Point2;

use crate::input::RoadInput;
use crate::mesh::{add_quad, MeshGroups};
use crate::ProceduralError;

pub fn generate(
    roads: &[RoadInput],
    include_sidewalks: bool,
) -> Result<MeshGroups, ProceduralError> {
    if roads.is_empty() {
        return Err(ProceduralError::InputMissing("roads"));
    }
    let mut groups = MeshGroups::default();
    for road in roads {
        validate(road)?;
        let road_edges = offset_edges(
            &road.centerline_enu_m,
            -road.width_m * 0.5,
            road.width_m * 0.5,
        )?;
        append_strip(
            groups.get_mut("roads", road.material_id.clone()),
            &road_edges.0,
            &road_edges.1,
            road.elevation_m + 0.025,
        )?;
        if include_sidewalks && road.sidewalk_width_m > 0.0 {
            let left = offset_edges(
                &road.centerline_enu_m,
                -road.width_m * 0.5 - road.sidewalk_width_m,
                -road.width_m * 0.5,
            )?;
            append_strip(
                groups.get_mut("sidewalks", road.sidewalk_material_id.clone()),
                &left.0,
                &left.1,
                road.elevation_m + 0.08,
            )?;
            let right = offset_edges(
                &road.centerline_enu_m,
                road.width_m * 0.5,
                road.width_m * 0.5 + road.sidewalk_width_m,
            )?;
            append_strip(
                groups.get_mut("sidewalks", road.sidewalk_material_id.clone()),
                &right.0,
                &right.1,
                road.elevation_m + 0.08,
            )?;
        }
    }
    Ok(groups)
}

fn validate(road: &RoadInput) -> Result<(), ProceduralError> {
    if road.centerline_enu_m.len() < 2
        || !road.width_m.is_finite()
        || road.width_m <= 0.5
        || !road.sidewalk_width_m.is_finite()
        || road.sidewalk_width_m < 0.0
        || !road.elevation_m.is_finite()
        || road
            .centerline_enu_m
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
    {
        return Err(ProceduralError::InvalidRoad(road.id.clone()));
    }
    if road
        .centerline_enu_m
        .windows(2)
        .any(|pair| distance(pair[0], pair[1]) < 1.0e-6)
    {
        return Err(ProceduralError::InvalidRoad(format!(
            "{}: vértices duplicados",
            road.id
        )));
    }
    Ok(())
}

fn offset_edges(
    line: &[Point2],
    left_offset: f64,
    right_offset: f64,
) -> Result<(Vec<Point2>, Vec<Point2>), ProceduralError> {
    let mut left = Vec::with_capacity(line.len());
    let mut right = Vec::with_capacity(line.len());
    for i in 0..line.len() {
        let previous = if i == 0 {
            direction(line[0], line[1])
        } else {
            direction(line[i - 1], line[i])
        };
        let next = if i + 1 == line.len() {
            direction(line[i - 1], line[i])
        } else {
            direction(line[i], line[i + 1])
        };
        let previous_normal = [-previous[1], previous[0]];
        let next_normal = [-next[1], next[0]];
        let sum = [
            previous_normal[0] + next_normal[0],
            previous_normal[1] + next_normal[1],
        ];
        let sum_length = sum[0].hypot(sum[1]);
        let miter = if sum_length < 1.0e-8 {
            next_normal
        } else {
            [sum[0] / sum_length, sum[1] / sum_length]
        };
        let denominator = (miter[0] * next_normal[0] + miter[1] * next_normal[1])
            .abs()
            .max(0.2);
        let left_scale =
            (left_offset / denominator).clamp(-left_offset.abs() * 4.0, left_offset.abs() * 4.0);
        let right_scale =
            (right_offset / denominator).clamp(-right_offset.abs() * 4.0, right_offset.abs() * 4.0);
        left.push([
            line[i][0] + miter[0] * left_scale,
            line[i][1] + miter[1] * left_scale,
        ]);
        right.push([
            line[i][0] + miter[0] * right_scale,
            line[i][1] + miter[1] * right_scale,
        ]);
    }
    Ok((left, right))
}

fn append_strip(
    primitive: &mut crate::mesh::Primitive,
    left: &[Point2],
    right: &[Point2],
    elevation: f64,
) -> Result<(), ProceduralError> {
    for i in 0..left.len() - 1 {
        let a = [left[i][0] as f32, elevation as f32, -left[i][1] as f32];
        let b = [right[i][0] as f32, elevation as f32, -right[i][1] as f32];
        let c = [
            right[i + 1][0] as f32,
            elevation as f32,
            -right[i + 1][1] as f32,
        ];
        let d = [
            left[i + 1][0] as f32,
            elevation as f32,
            -left[i + 1][1] as f32,
        ];
        add_quad(
            primitive,
            a,
            d,
            c,
            b,
            [distance(left[i], left[i + 1]) as f32 * 0.2, 1.0],
        )?;
    }
    Ok(())
}

fn direction(a: Point2, b: Point2) -> Point2 {
    let length = distance(a, b).max(1.0e-12);
    [(b[0] - a[0]) / length, (b[1] - a[1]) / length]
}
fn distance(a: Point2, b: Point2) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}
