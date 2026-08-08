//! Terreno em `quantized-mesh-1.0` gerado do nosso proprio DEM.
//!
//! O CesiumJS e Apache-2.0 e roda inteiro daqui (`vendor/cesium`), mas os
//! *dados* de relevo que a Cesium distribui — o World Terrain — sao servico do
//! Cesium ion, cobrado por uso. A regra R001 barra ligar isso sem autorizacao.
//!
//! O formato do terreno, porem, e aberto e esta publicado. Entao em vez de
//! consumir o servico, o ARCZ **produz** o mesmo formato a partir do DEM que ja
//! baixa (AWS Terrain Tiles, SRTM/GMTED2010) e serve pela propria porta. O
//! globo ganha relevo real, offline, sem custo e sem cadastro.
//!
//! Onde o mosaico nao alcanca, o tile sai plano no nivel do mar em vez de
//! faltar: um buraco no globo seria pior que um oceano liso, e e honesto —
//! aquela regiao nao foi baixada.
//!
//! Referencia do formato: <https://github.com/CesiumGS/quantized-mesh>

use arcz_geo::GeoBBox;

use crate::mosaic::HeightMosaic;

/// Vertices por aresta da grade de cada tile.
///
/// 32x32 da 1024 vertices e 1922 triangulos: cabe folgado no indice de 16 bits
/// (o limite do formato e 65536) e mantem o `.terrain` na casa dos 7 KB. Subir
/// para 64 quadruplicaria o trafego sem ganho visivel numa cidade, onde o
/// relevo entre dois vertices vizinhos ja e quase linear.
const LADO: usize = 32;

/// Maior valor de coordenada quantizada. O formato usa 15 bits uteis.
const MAX_Q: f64 = 32767.0;

// --- elipsoide WGS84 ------------------------------------------------------

const A: f64 = 6_378_137.0;
const F: f64 = 1.0 / 298.257_223_563;

/// Geodetico -> ECEF, o quadro em que o Cesium espera o bounding volume.
fn ecef(lon_deg: f64, lat_deg: f64, h: f64) -> [f64; 3] {
    let e2 = F * (2.0 - F);
    let (slat, clat) = lat_deg.to_radians().sin_cos();
    let (slon, clon) = lon_deg.to_radians().sin_cos();
    let n = A / (1.0 - e2 * slat * slat).sqrt();
    [
        (n + h) * clat * clon,
        (n + h) * clat * slon,
        (n * (1.0 - e2) + h) * slat,
    ]
}

/// Retangulo geografico do tile `(z, x, y)` no esquema **geografico** do Cesium.
///
/// Difere do Web Mercator usado pelo DEM: o nivel 0 tem dois tiles lado a lado
/// cobrindo o globo inteiro, e o eixo y cresce para o **norte** (TMS), nao para
/// o sul. Trocar isso deixaria o mundo de cabeca para baixo.
pub fn bounds_do_tile(z: u8, x: u32, y: u32) -> GeoBBox {
    let larg = 360.0 / f64::from(2u32.pow(u32::from(z) + 1));
    let alt = 180.0 / f64::from(2u32.pow(u32::from(z)));
    let west = -180.0 + f64::from(x) * larg;
    let south = -90.0 + f64::from(y) * alt;
    GeoBBox {
        west,
        south,
        east: west + larg,
        north: south + alt,
    }
}

/// Quantos tiles existem em cada eixo no nivel `z`.
pub fn tiles_no_nivel(z: u8) -> (u32, u32) {
    (2u32.pow(u32::from(z) + 1), 2u32.pow(u32::from(z)))
}

/// Codifica um tile de terreno amostrando `dem`, ou plano se `dem` for `None`.
///
/// `dem` cobre so a regiao carregada; fora dela o chamador passa `None` e o
/// tile sai no nivel do mar.
pub fn codificar(z: u8, x: u32, y: u32, dem: Option<&HeightMosaic>) -> Vec<u8> {
    let b = bounds_do_tile(z, x, y);

    // Amostra a grade. `alturas` vai do SUL para o NORTE porque e assim que o
    // eixo v do formato cresce; ler na ordem contraria espelharia o relevo.
    let mut alturas = vec![0.0f32; LADO * LADO];
    if let Some(dem) = dem {
        let cobre = dem.bounds();
        for j in 0..LADO {
            let t = j as f64 / (LADO - 1) as f64;
            let lat = b.south + (b.north - b.south) * t;
            for i in 0..LADO {
                let s = i as f64 / (LADO - 1) as f64;
                let lon = b.west + (b.east - b.west) * s;
                // Fora do mosaico `sample_geodetic` grampeia na borda, o que
                // esticaria o relevo do litoral por todo o oceano. Zero ali.
                alturas[j * LADO + i] = if lon >= cobre.west
                    && lon <= cobre.east
                    && lat >= cobre.south
                    && lat <= cobre.north
                {
                    dem.sample_geodetic(lon, lat)
                } else {
                    0.0
                };
            }
        }
    }

    let (mut h_min, mut h_max) = (f32::INFINITY, f32::NEG_INFINITY);
    for &h in &alturas {
        h_min = h_min.min(h);
        h_max = h_max.max(h);
    }
    let span = f64::from(h_max - h_min);

    // --- triangulos, em indices da grade ----------------------------------
    //
    // Winding anti-horario visto de fora da Terra. Como v cresce para o norte e
    // u para o leste, (k, k+1, k+LADO) ja sai nessa ordem.
    let mut grade: Vec<usize> = Vec::with_capacity((LADO - 1) * (LADO - 1) * 6);
    for j in 0..LADO - 1 {
        for i in 0..LADO - 1 {
            let k = j * LADO + i;
            grade.extend_from_slice(&[k, k + 1, k + LADO]);
            grade.extend_from_slice(&[k + 1, k + 1 + LADO, k + LADO]);
        }
    }

    // --- renumeracao por ordem de primeiro uso ----------------------------
    //
    // A codificacao de indices do formato so consegue escrever um indice que
    // seja no maximo o proximo ainda nao visto. Com a numeracao natural da
    // grade o primeiro triangulo ja cita o vertice LADO, que viola isso. Entao
    // os vertices sao renumerados na ordem em que os triangulos os citam, e o
    // buffer e reordenado junto.
    let n = LADO * LADO;
    let mut novo_de = vec![u32::MAX; n];
    let mut ordem: Vec<usize> = Vec::with_capacity(n);
    let mut indices: Vec<u32> = Vec::with_capacity(grade.len());
    for &k in &grade {
        if novo_de[k] == u32::MAX {
            novo_de[k] = ordem.len() as u32;
            ordem.push(k);
        }
        indices.push(novo_de[k]);
    }
    debug_assert_eq!(ordem.len(), n, "a grade deve citar todos os vertices");

    // --- vertices quantizados --------------------------------------------
    let (mut us, mut vs, mut hs) = (vec![0u16; n], vec![0u16; n], vec![0u16; n]);
    for (novo, &k) in ordem.iter().enumerate() {
        let (i, j) = (k % LADO, k / LADO);
        us[novo] = (i as f64 / (LADO - 1) as f64 * MAX_Q).round() as u16;
        vs[novo] = (j as f64 / (LADO - 1) as f64 * MAX_Q).round() as u16;
        // Tile plano tem span zero: qualquer divisao ali daria NaN.
        hs[novo] = if span > 1e-6 {
            (f64::from(alturas[k] - h_min) / span * MAX_Q).round() as u16
        } else {
            0
        };
    }

    // --- volume delimitador ----------------------------------------------
    //
    // O Cesium usa a esfera para descartar o tile fora de vista. Ela precisa
    // conter a superficie inteira, entao os cantos entram com a altura maxima.
    let cantos = [
        ecef(b.west, b.south, f64::from(h_max)),
        ecef(b.east, b.south, f64::from(h_max)),
        ecef(b.west, b.north, f64::from(h_max)),
        ecef(b.east, b.north, f64::from(h_max)),
        ecef(b.west, b.south, f64::from(h_min)),
        ecef(b.east, b.north, f64::from(h_min)),
    ];
    let mut centro = [0.0f64; 3];
    for c in &cantos {
        for e in 0..3 {
            centro[e] += c[e] / cantos.len() as f64;
        }
    }
    let mut raio: f64 = 0.0;
    for c in &cantos {
        let d = ((c[0] - centro[0]).powi(2) + (c[1] - centro[1]).powi(2) + (c[2] - centro[2]).powi(2))
            .sqrt();
        raio = raio.max(d);
    }

    let mut out = Vec::with_capacity(8 * 1024);

    // --- cabecalho --------------------------------------------------------
    for e in centro {
        out.extend_from_slice(&e.to_le_bytes());
    }
    out.extend_from_slice(&h_min.to_le_bytes());
    out.extend_from_slice(&h_max.to_le_bytes());
    for e in centro {
        out.extend_from_slice(&e.to_le_bytes());
    }
    out.extend_from_slice(&raio.to_le_bytes());

    // Ponto de oclusao pelo horizonte. Calcular o valor exato so renderiza
    // ganho de culling; um ponto bem afastado na direcao do centro e sempre
    // *visivel*, entao o tile nunca e descartado por engano — pior desempenho,
    // nunca buraco na tela. Trocar por conta exata e otimizacao, nao correcao.
    let norma = (centro[0].powi(2) + centro[1].powi(2) + centro[2].powi(2)).sqrt();
    for e in centro {
        out.extend_from_slice(&(e / norma * 2.0).to_le_bytes());
    }

    // --- vertices ---------------------------------------------------------
    out.extend_from_slice(&(n as u32).to_le_bytes());
    for arr in [&us, &vs, &hs] {
        let mut anterior = 0i32;
        for &v in arr.iter() {
            let d = i32::from(v) - anterior;
            anterior = i32::from(v);
            // Zigzag: mapeia negativos para impares para caber em u16.
            out.extend_from_slice(&((((d << 1) ^ (d >> 31)) as u16).to_le_bytes()));
        }
    }

    // --- triangulos -------------------------------------------------------
    out.extend_from_slice(&((indices.len() / 3) as u32).to_le_bytes());
    // Codificacao por marca d'agua: cada indice vira a distancia ate o maior ja
    // visto, o que concentra os valores perto de zero e comprime bem.
    let mut alto: u32 = 0;
    for &i in &indices {
        let code = alto - i;
        out.extend_from_slice(&(code as u16).to_le_bytes());
        if code == 0 {
            alto += 1;
        }
    }

    // --- bordas -----------------------------------------------------------
    //
    // Dizem ao Cesium quais vertices costuram com o tile vizinho. Sem isso
    // aparecem fendas entre tiles de niveis diferentes.
    // Os indices sao os **renumerados**; citar a numeracao da grade aqui
    // costuraria os tiles pelos vertices errados.
    let bd = |k: usize| novo_de[k] as u16;
    let oeste: Vec<u16> = (0..LADO).map(|j| bd(j * LADO)).collect();
    let sul: Vec<u16> = (0..LADO).map(bd).collect();
    let leste: Vec<u16> = (0..LADO).map(|j| bd(j * LADO + LADO - 1)).collect();
    let norte: Vec<u16> = (0..LADO).map(|i| bd((LADO - 1) * LADO + i)).collect();
    for borda in [&oeste, &sul, &leste, &norte] {
        out.extend_from_slice(&(borda.len() as u32).to_le_bytes());
        for &i in borda.iter() {
            out.extend_from_slice(&i.to_le_bytes());
        }
    }

    out
}

/// O `layer.json` que o `CesiumTerrainProvider` le antes de pedir tiles.
///
/// Declara o mundo inteiro ate `nivel_base` (plano fora da regiao) e, dali para
/// cima, apenas o retangulo coberto pelo DEM. Anunciar detalhe onde nao ha DEM
/// so faria o Cesium baixar tiles planos cada vez menores.
pub fn layer_json(cobertura: Option<GeoBBox>, nivel_base: u8, nivel_max: u8) -> String {
    let mut niveis = Vec::new();
    for z in 0..=nivel_max {
        let (nx, ny) = tiles_no_nivel(z);
        let faixa = if z <= nivel_base {
            (0, 0, nx - 1, ny - 1)
        } else if let Some(c) = cobertura {
            let larg = 360.0 / f64::from(nx);
            let alt = 180.0 / f64::from(ny);
            let ix = |lon: f64| ((lon + 180.0) / larg).floor().clamp(0.0, f64::from(nx - 1)) as u32;
            let iy = |lat: f64| ((lat + 90.0) / alt).floor().clamp(0.0, f64::from(ny - 1)) as u32;
            (ix(c.west), iy(c.south), ix(c.east), iy(c.north))
        } else {
            continue;
        };
        niveis.push(format!(
            r#"[{{"startX":{},"startY":{},"endX":{},"endY":{}}}]"#,
            faixa.0, faixa.1, faixa.2, faixa.3
        ));
    }
    format!(
        r#"{{"tilejson":"2.1.0","name":"ARCZ","description":"Relevo do DEM local (AWS Terrain Tiles)","version":"1.0.0","format":"quantized-mesh-1.0","attribution":"Mapzen/AWS Terrain Tiles","scheme":"tms","tiles":["{{z}}/{{x}}/{{y}}.terrain"],"projection":"EPSG:4326","bounds":[-180,-90,180,90],"available":[{}]}}"#,
        niveis.join(",")
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nivel_zero_cobre_o_globo_em_dois_tiles() {
        assert_eq!(tiles_no_nivel(0), (2, 1));
        let oeste = bounds_do_tile(0, 0, 0);
        let leste = bounds_do_tile(0, 1, 0);
        assert_eq!((oeste.west, oeste.east), (-180.0, 0.0));
        assert_eq!((leste.west, leste.east), (0.0, 180.0));
        assert_eq!((oeste.south, oeste.north), (-90.0, 90.0));
    }

    #[test]
    fn y_cresce_para_o_norte() {
        // TMS, ao contrario do XYZ do DEM. Se algum dia isso inverter, o globo
        // aparece espelhado no eixo vertical — dai o teste.
        let baixo = bounds_do_tile(1, 0, 0);
        let cima = bounds_do_tile(1, 0, 1);
        assert!(cima.south > baixo.south);
        assert_eq!(baixo.south, -90.0);
        assert_eq!(cima.north, 90.0);
    }

    #[test]
    fn tile_plano_tem_o_tamanho_previsto_pelo_formato() {
        let t = codificar(0, 0, 0, None);
        let n = LADO * LADO;
        let tris = (LADO - 1) * (LADO - 1) * 2;
        let esperado = 88                    // cabecalho
            + 4 + n * 2 * 3                  // contagem + u, v, height
            + 4 + tris * 3 * 2               // contagem + indices de 16 bits
            + 4 * 4 + LADO * 2 * 4; // quatro bordas
        assert_eq!(t.len(), esperado);
    }

    #[test]
    fn cabecalho_traz_min_e_max_de_um_tile_plano() {
        let t = codificar(3, 2, 1, None);
        let h_min = f32::from_le_bytes(t[24..28].try_into().unwrap());
        let h_max = f32::from_le_bytes(t[28..32].try_into().unwrap());
        assert_eq!((h_min, h_max), (0.0, 0.0));
        // Raio precisa ser positivo, senao o Cesium descarta o tile sempre.
        let raio = f64::from_le_bytes(t[56..64].try_into().unwrap());
        assert!(raio > 0.0, "raio {raio}");
    }

    #[test]
    fn indices_por_marca_dagua_reconstroem_a_lista_original() {
        // Decodifica o proprio arquivo e confere que os triangulos voltam.
        let t = codificar(0, 0, 0, None);
        let n = LADO * LADO;
        let base = 88 + 4 + n * 2 * 3;
        let tris = u32::from_le_bytes(t[base..base + 4].try_into().unwrap()) as usize;
        assert_eq!(tris, (LADO - 1) * (LADO - 1) * 2);

        let mut alto: u32 = 0;
        let mut lidos = Vec::with_capacity(tris * 3);
        for k in 0..tris * 3 {
            let o = base + 4 + k * 2;
            let code = u32::from(u16::from_le_bytes(t[o..o + 2].try_into().unwrap()));
            let i = alto - code;
            if code == 0 {
                alto += 1;
            }
            lidos.push(i);
        }
        // Com a renumeracao por primeiro uso, o triangulo inicial cita os tres
        // primeiros vertices do buffer.
        assert_eq!(&lidos[..3], &[0, 1, 2]);
        assert!(lidos.iter().all(|&i| (i as usize) < n));
        // E todo vertice do buffer e usado por algum triangulo.
        let usados: std::collections::HashSet<u32> = lidos.iter().copied().collect();
        assert_eq!(usados.len(), n);
    }

    #[test]
    fn layer_json_limita_o_detalhe_a_regiao_carregada() {
        let c = GeoBBox {
            west: -48.51,
            south: -27.16,
            east: -48.49,
            north: -27.14,
        };
        let j = layer_json(Some(c), 5, 12);
        // Ate o nivel base o mundo inteiro; no nivel 12 so uns poucos tiles.
        assert!(j.contains(r#"{"startX":0,"startY":0,"endX":1,"endY":0}"#));
        let (nx, _) = tiles_no_nivel(12);
        let ix = ((-48.51 + 180.0) / (360.0 / f64::from(nx))).floor() as u32;
        assert!(j.contains(&format!(r#"{{"startX":{ix},"#)), "faltou o tile da regiao: {j}");
        assert!(j.contains("quantized-mesh-1.0"));
    }

    #[test]
    fn sem_cobertura_so_anuncia_o_nivel_base() {
        let j = layer_json(None, 3, 12);
        assert_eq!(j.matches("startX").count(), 4); // niveis 0..=3
    }
}
