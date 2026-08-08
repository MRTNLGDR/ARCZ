use crate::input::{FlatTerrainFallback, TerrainGrid};
use crate::mesh::Primitive;
use crate::ProceduralError;

pub fn from_flat(fallback: &FlatTerrainFallback) -> Result<TerrainGrid, ProceduralError> {
    let [west, south, east, north] = fallback.bounds_enu_m;
    if !(west.is_finite() && south.is_finite() && east.is_finite() && north.is_finite())
        || west >= east || south >= north || !fallback.elevation_m.is_finite() {
        return Err(ProceduralError::InvalidTerrain("bounds/elevation inválidos".to_owned()));
    }
    let resolution = fallback.resolution.clamp(2, 4096);
    let columns = resolution;
    let rows = resolution;
    Ok(TerrainGrid {
        origin_enu_m: [west, south], columns, rows,
        cell_size_m: [(east - west) / (columns - 1) as f64, (north - south) / (rows - 1) as f64],
        heights_m: vec![fallback.elevation_m as f32; columns * rows],
        material_id: fallback.material_id.clone(),
    })
}

pub fn generate(grid: &TerrainGrid) -> Result<Primitive, ProceduralError> {
    if grid.columns < 2 || grid.rows < 2 || grid.columns > 8192 || grid.rows > 8192
        || grid.columns.checked_mul(grid.rows) != Some(grid.heights_m.len())
        || grid.cell_size_m.iter().any(|value| !value.is_finite() || *value <= 0.0)
        || grid.heights_m.iter().any(|value| !value.is_finite()) {
        return Err(ProceduralError::InvalidTerrain("dimensões, espaçamento ou alturas inválidas".to_owned()));
    }
    let vertex_count = grid.columns.checked_mul(grid.rows).ok_or(ProceduralError::GeometryTooLarge)?;
    if vertex_count > 16_777_216 { return Err(ProceduralError::GeometryTooLarge); }
    let mut primitive = Primitive {
        name: "terrain".to_owned(), material_id: grid.material_id.clone(),
        positions: Vec::with_capacity(vertex_count), normals: Vec::with_capacity(vertex_count),
        uvs: Vec::with_capacity(vertex_count),
        indices: Vec::with_capacity((grid.columns - 1) * (grid.rows - 1) * 6),
        extras: serde_json::json!({"coordinate_system":"ENU_LOCAL","source":"explicit_grid"}),
    };
    for row in 0..grid.rows {
        for column in 0..grid.columns {
            let x = grid.origin_enu_m[0] + column as f64 * grid.cell_size_m[0];
            let north = grid.origin_enu_m[1] + row as f64 * grid.cell_size_m[1];
            let height = grid.heights_m[row * grid.columns + column];
            primitive.positions.push([x as f32, height, -north as f32]);
            primitive.normals.push(normal_at(grid, column, row));
            primitive.uvs.push([
                column as f32 / (grid.columns - 1) as f32,
                row as f32 / (grid.rows - 1) as f32,
            ]);
        }
    }
    for row in 0..grid.rows - 1 {
        for column in 0..grid.columns - 1 {
            let a = u32::try_from(row * grid.columns + column).map_err(|_| ProceduralError::GeometryTooLarge)?;
            let b = a + 1;
            let d = u32::try_from((row + 1) * grid.columns + column).map_err(|_| ProceduralError::GeometryTooLarge)?;
            let c = d + 1;
            primitive.indices.extend([a, b, c, a, c, d]);
        }
    }
    Ok(primitive)
}

fn normal_at(grid: &TerrainGrid, column: usize, row: usize) -> [f32; 3] {
    let left = grid.heights_m[row * grid.columns + column.saturating_sub(1)] as f64;
    let right = grid.heights_m[row * grid.columns + (column + 1).min(grid.columns - 1)] as f64;
    let down = grid.heights_m[row.saturating_sub(1) * grid.columns + column] as f64;
    let up = grid.heights_m[(row + 1).min(grid.rows - 1) * grid.columns + column] as f64;
    let dx_den = if column == 0 || column + 1 == grid.columns { grid.cell_size_m[0] } else { 2.0 * grid.cell_size_m[0] };
    let dy_den = if row == 0 || row + 1 == grid.rows { grid.cell_size_m[1] } else { 2.0 * grid.cell_size_m[1] };
    let dh_dx = (right - left) / dx_den;
    let dh_dy = (up - down) / dy_den;
    let mut normal = [-dh_dx, 1.0, dh_dy];
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt().max(1.0e-12);
    normal.iter_mut().for_each(|value| *value /= length);
    [normal[0] as f32, normal[1] as f32, normal[2] as f32]
}
