//! Construcao da malha de terreno no quadro ENU local.
//!
//! A grade e uniforme em **Web Mercator**, nao em graus. Isso faz cada vertice cair
//! num passo constante de pixels do DEM e da textura, o que evita amostragem irregular
//! e deixa a UV exatamente linear.

use arcz_geo::tiles::{lat_to_mercator_y, mercator_y_to_lat_deg};
use arcz_geo::{EnuFrame, GeoBBox, Geodetic};

use crate::mosaic::{HeightMosaic, ImageMosaic};

/// Vertice pronto para a GPU. `position` ja esta em ENU local (`f32` seguro).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct TerrainVertex {
    /// x = leste, y = cima, z = -norte, em metros relativos a origem do quadro.
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// Malha de terreno com a caixa envolvente em ENU.
#[derive(Debug, Clone)]
pub struct TerrainMesh {
    pub vertices: Vec<TerrainVertex>,
    pub indices: Vec<u32>,
    /// Grade `grid_n` x `grid_n` de vertices.
    pub grid_n: u32,
    pub min_enu: [f32; 3],
    pub max_enu: [f32; 3],
}

impl TerrainMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Centro da caixa envolvente, em coordenadas de render.
    pub fn center(&self) -> [f32; 3] {
        [
            (self.min_enu[0] + self.max_enu[0]) * 0.5,
            (self.min_enu[1] + self.max_enu[1]) * 0.5,
            (self.min_enu[2] + self.max_enu[2]) * 0.5,
        ]
    }

    /// Maior dimensao horizontal, usada para enquadrar a camera.
    pub fn horizontal_extent(&self) -> f32 {
        (self.max_enu[0] - self.min_enu[0]).max(self.max_enu[2] - self.min_enu[2])
    }
}

/// Gera a malha que cobre `target`, amostrando `dem` e mapeando UV sobre `imagery`.
///
/// `grid_n` e o numero de vertices por eixo (minimo 2).
pub fn build(
    target: &GeoBBox,
    grid_n: u32,
    dem: &HeightMosaic,
    imagery: &ImageMosaic,
    frame: &EnuFrame,
) -> TerrainMesh {
    let n = grid_n.max(2);
    let nf = (n - 1) as f64;

    // Grade uniforme em Mercator: interpola Y projetado, nao a latitude.
    let my_north = lat_to_mercator_y(target.north);
    let my_south = lat_to_mercator_y(target.south);

    let mut vertices = Vec::with_capacity((n * n) as usize);
    let mut min_enu = [f32::INFINITY; 3];
    let mut max_enu = [f32::NEG_INFINITY; 3];

    for j in 0..n {
        let ty = j as f64 / nf;
        let lat = mercator_y_to_lat_deg(my_north + (my_south - my_north) * ty);

        for i in 0..n {
            let tx = i as f64 / nf;
            let lon = target.west + (target.east - target.west) * tx;

            let h = dem.sample_geodetic(lon, lat) as f64;
            let enu = frame.geodetic_to_enu(Geodetic::new(lon, lat, h));
            let position = enu.to_render_f32();

            for k in 0..3 {
                min_enu[k] = min_enu[k].min(position[k]);
                max_enu[k] = max_enu[k].max(position[k]);
            }

            vertices.push(TerrainVertex {
                position,
                // Preenchida no segundo passo, quando os vizinhos ja existem.
                normal: [0.0, 1.0, 0.0],
                uv: imagery.uv_for(lon, lat),
            });
        }
    }

    calcular_normais(&mut vertices, n);

    // Dois triangulos por celula. A ordem abaixo e anti-horaria vista de cima
    // (+Y), que e o `front_face: Ccw` do pipeline em arcz-app.
    let mut indices = Vec::with_capacity(((n - 1) * (n - 1) * 6) as usize);
    for j in 0..n - 1 {
        for i in 0..n - 1 {
            let v00 = j * n + i;
            let v10 = v00 + 1;
            let v01 = v00 + n;
            let v11 = v01 + 1;
            indices.extend_from_slice(&[v00, v01, v11, v00, v11, v10]);
        }
    }

    TerrainMesh {
        vertices,
        indices,
        grid_n: n,
        min_enu,
        max_enu,
    }
}

/// Normais por diferenca central dos vizinhos na grade.
fn calcular_normais(vertices: &mut [TerrainVertex], n: u32) {
    let idx = |i: u32, j: u32| (j * n + i) as usize;

    let posicoes: Vec<[f32; 3]> = vertices.iter().map(|v| v.position).collect();

    for j in 0..n {
        for i in 0..n {
            let leste = posicoes[idx((i + 1).min(n - 1), j)];
            let oeste = posicoes[idx(i.saturating_sub(1), j)];
            let sul = posicoes[idx(i, (j + 1).min(n - 1))];
            let norte = posicoes[idx(i, j.saturating_sub(1))];

            // du aponta para leste (+x), dv aponta para o sul (+z em render).
            let du = sub(leste, oeste);
            let dv = sub(sul, norte);
            // dv x du da +Y num terreno plano — mesma orientacao dos triangulos.
            let nrm = normalize(cross(dv, du));
            vertices[idx(i, j)].normal = nrm;
        }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-12 {
        [0.0, 1.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcz_geo::{TileId, TileRange};

    const CENTRO: Geodetic = Geodetic::new(-46.633_308, -23.550_520, 0.0);

    fn cenario(altura: impl Fn(u32, u32) -> f32) -> (GeoBBox, HeightMosaic, ImageMosaic, EnuFrame) {
        let z = 12;
        let bbox = GeoBBox::around(CENTRO, 3_000.0).unwrap();
        let range = TileRange::covering(&bbox, z);
        let ts = 32;

        let cols = range.x_max - range.x_min + 1;
        let tiles: Vec<_> = range
            .iter()
            .map(|id| {
                let ox = (id.x - range.x_min) * ts;
                let oy = (id.y - range.y_min) * ts;
                let dados: Vec<f32> = (0..ts * ts)
                    .map(|k| altura(ox + (k % ts), oy + (k / ts)))
                    .collect();
                (id, Some(dados))
            })
            .collect();

        let dem = HeightMosaic::from_tiles(range, ts, tiles);
        let px = (cols * ts) as usize * ((range.y_max - range.y_min + 1) * ts) as usize;
        let img = ImageMosaic::from_raw(range, ts, vec![255u8; px * 4]);
        let frame = EnuFrame::new(CENTRO);

        (bbox, dem, img, frame)
    }

    #[test]
    fn contagem_de_vertices_e_indices_bate_com_a_grade() {
        let (bbox, dem, img, frame) = cenario(|_, _| 700.0);
        let n = 33;
        let m = build(&bbox, n, &dem, &img, &frame);

        assert_eq!(m.grid_n, n);
        assert_eq!(m.vertices.len(), (n * n) as usize);
        assert_eq!(m.indices.len(), ((n - 1) * (n - 1) * 6) as usize);
        assert_eq!(m.triangle_count(), ((n - 1) * (n - 1) * 2) as usize);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.vertices.len()));
    }

    #[test]
    fn grid_n_minimo_e_dois() {
        let (bbox, dem, img, frame) = cenario(|_, _| 0.0);
        let m = build(&bbox, 0, &dem, &img, &frame);
        assert_eq!(m.grid_n, 2);
        assert_eq!(m.triangle_count(), 2);
    }

    #[test]
    fn nenhum_vertice_tem_nan_ou_infinito() {
        let (bbox, dem, img, frame) = cenario(|x, y| (x as f32).sin() * 300.0 + (y as f32) * 0.7);
        let m = build(&bbox, 65, &dem, &img, &frame);

        for v in &m.vertices {
            for c in v.position.iter().chain(v.normal.iter()).chain(v.uv.iter()) {
                assert!(c.is_finite(), "componente nao finita em {v:?}");
            }
            let len = (v.normal[0].powi(2) + v.normal[1].powi(2) + v.normal[2].powi(2)).sqrt();
            assert!((len - 1.0).abs() < 1e-4, "normal nao unitaria: {len}");
        }
    }

    /// Winding: com terreno plano, o produto vetorial de cada triangulo tem que
    /// apontar para cima. Se este teste falhar, o terreno some por back-face culling.
    #[test]
    fn triangulos_sao_ccw_vistos_de_cima() {
        let (bbox, dem, img, frame) = cenario(|_, _| 800.0);
        let m = build(&bbox, 9, &dem, &img, &frame);

        for t in m.indices.chunks_exact(3) {
            let a = m.vertices[t[0] as usize].position;
            let b = m.vertices[t[1] as usize].position;
            let c = m.vertices[t[2] as usize].position;
            let nrm = cross(sub(b, a), sub(c, a));
            assert!(nrm[1] > 0.0, "triangulo {t:?} virado para baixo: {nrm:?}");
        }
    }

    #[test]
    fn terreno_plano_tem_normal_apontando_para_cima() {
        let (bbox, dem, img, frame) = cenario(|_, _| 760.0);
        let m = build(&bbox, 17, &dem, &img, &frame);

        for v in &m.vertices {
            // A curvatura da Terra em 3 km inclina o plano tangente em ~2.4e-4 rad.
            assert!(
                v.normal[1] > 0.999,
                "normal {:?} nao aponta para cima",
                v.normal
            );
        }
    }

    /// Rampa subindo para o leste: a normal tem que se inclinar para o OESTE (-x).
    /// Trocar o sinal aqui deixa a iluminacao invertida — erro silencioso e caro.
    #[test]
    fn rampa_para_leste_inclina_a_normal_para_oeste() {
        let (bbox, dem, img, frame) = cenario(|x, _| x as f32 * 20.0);
        let m = build(&bbox, 17, &dem, &img, &frame);

        let centro = &m.vertices[(17 * 17 / 2) as usize];
        assert!(
            centro.normal[0] < -0.05,
            "normal deveria pender para -x, deu {:?}",
            centro.normal
        );
        assert!(centro.normal[1] > 0.0);
    }

    #[test]
    fn a_malha_reproduz_as_alturas_do_dem() {
        // Faixas norte-sul de 8 px alternando 0 m e 1000 m. O periodo (16 px) e menor
        // que a janela que a bbox recorta do mosaico, entao a malha necessariamente
        // atravessa varias transicoes — independente de onde a bbox caia nos tiles.
        let (bbox, dem, img, frame) = cenario(|x, _| ((x / 8) % 2) as f32 * 1000.0);
        let m = build(&bbox, 65, &dem, &img, &frame);

        let alturas: Vec<f32> = m.vertices.iter().map(|v| v.position[1]).collect();
        let lo = alturas.iter().cloned().fold(f32::INFINITY, f32::min);
        let hi = alturas.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // A malha nao pode achatar o relevo.
        assert!(hi - lo > 100.0, "relevo achatado: {lo} .. {hi}");
        // Nem inventar altura fora do que o DEM contem. A margem inferior de -2 m
        // acomoda a queda do plano tangente pela curvatura da Terra.
        assert!(
            hi <= 1001.0 && lo >= -2.0,
            "alturas fora do DEM: {lo} .. {hi}"
        );
    }

    /// A prova anti-jitter no nivel da malha: todo vertice tem que caber com folga
    /// na precisao do `f32`.
    #[test]
    fn coordenadas_da_malha_ficam_na_faixa_segura_do_f32() {
        let (bbox, dem, img, frame) = cenario(|_, _| 760.0);
        let m = build(&bbox, 65, &dem, &img, &frame);

        let maior = m
            .vertices
            .iter()
            .flat_map(|v| v.position)
            .fold(0.0_f32, |acc, c| acc.max(c.abs()));

        assert!(
            (maior as f64) < EnuFrame::REBASE_THRESHOLD_M,
            "vertice a {maior} m da origem, acima do limite de rebase"
        );
        // ulp do f32 nessa magnitude tem que ser submilimetrico.
        let ulp = (maior.max(1.0) as f64) * f32::EPSILON as f64;
        assert!(ulp < 1e-3, "resolucao do f32 nessa escala: {ulp} m");
    }

    #[test]
    fn uv_cobre_a_textura_sem_estourar_a_faixa() {
        let (bbox, dem, img, frame) = cenario(|_, _| 0.0);
        let m = build(&bbox, 33, &dem, &img, &frame);

        for v in &m.vertices {
            assert!(
                (-0.001..=1.001).contains(&v.uv[0]) && (-0.001..=1.001).contains(&v.uv[1]),
                "uv fora de [0,1]: {:?}",
                v.uv
            );
        }

        // O primeiro vertice e o canto noroeste; o ultimo, o sudeste.
        let primeiro = m.vertices[0].uv;
        let ultimo = m.vertices[m.vertices.len() - 1].uv;
        assert!(primeiro[0] < ultimo[0], "u nao cresce para leste");
        assert!(primeiro[1] < ultimo[1], "v nao cresce para o sul");
    }

    #[test]
    fn a_malha_cobre_a_bbox_pedida() {
        let (bbox, dem, img, frame) = cenario(|_, _| 0.0);
        let m = build(&bbox, 9, &dem, &img, &frame);
        let frame_ref = EnuFrame::new(CENTRO);

        let so = frame_ref
            .geodetic_to_enu(Geodetic::new(bbox.west, bbox.south, 0.0))
            .to_render_f32();
        let ne = frame_ref
            .geodetic_to_enu(Geodetic::new(bbox.east, bbox.north, 0.0))
            .to_render_f32();

        assert!((m.min_enu[0] - so[0]).abs() < 1.0, "borda oeste");
        assert!((m.max_enu[0] - ne[0]).abs() < 1.0, "borda leste");
        // z = -norte, entao o NORTE e o menor z.
        assert!((m.min_enu[2] - ne[2]).abs() < 1.0, "borda norte");
        assert!((m.max_enu[2] - so[2]).abs() < 1.0, "borda sul");

        let extensao = m.horizontal_extent();
        assert!(
            (extensao - 3_000.0).abs() < 60.0,
            "extensao horizontal {extensao} m, esperado ~3000"
        );
    }

    #[test]
    fn tile_id_do_centro_esta_dentro_da_faixa_usada() {
        // Guarda simples contra a bbox e o DEM se referirem a lugares diferentes.
        let (bbox, dem, _, _) = cenario(|_, _| 0.0);
        let c = bbox.center();
        let t = TileId::from_geodetic(c, dem.range().z);
        let r = dem.range();
        assert!(t.x >= r.x_min && t.x <= r.x_max && t.y >= r.y_min && t.y <= r.y_max);
    }
}
