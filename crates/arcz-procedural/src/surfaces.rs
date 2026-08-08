use crate::geometry::{ensure_ccw, render_point, triangulate};
use crate::input::ParcelInput;
use crate::mesh::{add_triangle, MeshGroups};
use crate::ProceduralError;

pub fn generate(parcels: &[ParcelInput]) -> Result<MeshGroups, ProceduralError> {
    if parcels.is_empty() {
        return Err(ProceduralError::InputMissing("parcels"));
    }
    let mut groups = MeshGroups::default();
    for parcel in parcels {
        let polygon = ensure_ccw(&parcel.polygon_enu_m)?;
        let triangles = triangulate(&polygon)?;
        let primitive = groups.get_mut("parcels", parcel.material_id.clone());
        for triangle in triangles {
            let a = polygon[triangle[0]];
            let b = polygon[triangle[1]];
            let c = polygon[triangle[2]];
            add_triangle(
                primitive,
                render_point(a, parcel.elevation_m + 0.015)?,
                render_point(b, parcel.elevation_m + 0.015)?,
                render_point(c, parcel.elevation_m + 0.015)?,
                [
                    [a[0] as f32 * 0.1, a[1] as f32 * 0.1],
                    [b[0] as f32 * 0.1, b[1] as f32 * 0.1],
                    [c[0] as f32 * 0.1, c[1] as f32 * 0.1],
                ],
            )?;
        }
    }
    Ok(groups)
}
