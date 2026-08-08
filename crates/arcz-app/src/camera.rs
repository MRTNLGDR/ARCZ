//! Camera orbital com matematica em `f64`.
//!
//! A matriz view-projection e montada inteira em `f64` e so o resultado final vira
//! `f32`. Como os vertices ja chegam em ENU local, o produto `view * posicao` nunca
//! envolve numeros grandes — que e exatamente a condicao para nao haver jitter.

/// Matriz 4x4 em ordem de coluna (column-major), compativel com WGSL.
pub type Mat4 = [[f64; 4]; 4];

#[derive(Debug, Clone, Copy)]
pub struct OrbitCamera {
    /// Ponto observado, em coordenadas de render (x=leste, y=cima, z=-norte).
    pub alvo: [f64; 3],
    pub distancia: f64,
    /// Rotacao em torno do eixo vertical, em radianos.
    pub yaw: f64,
    /// Elevacao acima do horizonte, em radianos.
    pub pitch: f64,
    /// Campo de visao vertical, em radianos.
    pub fov_y: f64,
    pub near: f64,
    pub far: f64,
}

impl OrbitCamera {
    /// Enquadra uma caixa de lado `extensao` centrada em `alvo`.
    ///
    /// A distancia sai da esfera envolvente, nao de um multiplicador chutado:
    /// `d = R / sin(fov/2)` poe a esfera de raio `R` exatamente tangente ao frustum.
    /// `R` usa a meia-diagonal (`lado * 0.71`) com folga, porque numa vista obliqua os
    /// cantos proximos da camera sao os primeiros a sair da tela — e um multiplicador
    /// como `1.1 * lado` corta justamente esses.
    pub fn enquadrando(alvo: [f32; 3], extensao: f32) -> Self {
        let extensao = extensao.max(1.0) as f64;
        let fov_y = 45f64.to_radians();
        // 0.62 e nao 0.71 (meia-diagonal exata): a cena e um plano visto de vies, nao
        // uma esfera, entao a esfera envolvente exagera. Este valor enche a tela sem
        // cortar os cantos — o teste `enquadramento_cobre_a_extensao_da_cena` trava isso.
        let raio = extensao * 0.62;
        Self {
            alvo: [alvo[0] as f64, alvo[1] as f64, alvo[2] as f64],
            distancia: raio / (fov_y / 2.0).sin(),
            yaw: -0.6,
            pitch: 0.45,
            fov_y,
            // `near` proporcional a cena evita z-fighting sem precisar de depth reverso
            // nesta fatia. Terreno de dezenas de km entra na Fatia 2 com far plane
            // logaritmico.
            near: (extensao * 0.001).max(0.5),
            far: extensao * 20.0,
        }
    }

    pub fn posicao(&self) -> [f64; 3] {
        let cp = self.pitch.cos();
        [
            self.alvo[0] + self.distancia * cp * self.yaw.sin(),
            self.alvo[1] + self.distancia * self.pitch.sin(),
            self.alvo[2] + self.distancia * cp * self.yaw.cos(),
        ]
    }

    pub fn orbitar(&mut self, d_yaw: f64, d_pitch: f64) {
        self.yaw += d_yaw;
        // Trava um pouco antes dos polos: em ±90° o vetor "up" degenera e a view
        // matrix vira NaN.
        const LIMITE: f64 = 1.553; // ~89°
        self.pitch = (self.pitch + d_pitch).clamp(-LIMITE, LIMITE);
    }

    pub fn zoom(&mut self, fator: f64) {
        self.distancia = (self.distancia * fator).clamp(self.near * 4.0, self.far * 0.5);
    }

    pub fn view_proj(&self, aspecto: f64) -> Mat4 {
        mul(self.proj(aspecto), self.view())
    }

    pub fn view(&self) -> Mat4 {
        look_at(self.posicao(), self.alvo, [0.0, 1.0, 0.0])
    }

    /// Projecao perspectiva com profundidade em `[0, 1]` (convencao de wgpu/D3D/Vulkan,
    /// **nao** a de OpenGL, que usa `[-1, 1]`).
    pub fn proj(&self, aspecto: f64) -> Mat4 {
        let f = 1.0 / (self.fov_y / 2.0).tan();
        let mut m = [[0.0; 4]; 4];
        m[0][0] = f / aspecto.max(1e-6);
        m[1][1] = f;
        m[2][2] = self.far / (self.near - self.far);
        m[2][3] = -1.0;
        m[3][2] = self.near * self.far / (self.near - self.far);
        m
    }
}

/// Inversa de uma matriz 4x4 geral (cofatores).
///
/// O shader de ceu precisa dela para reconstruir a direcao do raio de cada pixel a
/// partir do NDC. Devolve identidade se a matriz for singular — melhor um ceu chapado
/// do que NaN espalhado pela tela.
pub fn inverse(m: Mat4) -> Mat4 {
    // Trabalha em ordem linear (coluna-major, igual ao resto do modulo).
    let a: [f64; 16] = [
        m[0][0], m[0][1], m[0][2], m[0][3], m[1][0], m[1][1], m[1][2], m[1][3], m[2][0], m[2][1],
        m[2][2], m[2][3], m[3][0], m[3][1], m[3][2], m[3][3],
    ];

    let mut inv = [0.0f64; 16];
    inv[0] = a[5] * a[10] * a[15] - a[5] * a[11] * a[14] - a[9] * a[6] * a[15]
        + a[9] * a[7] * a[14]
        + a[13] * a[6] * a[11]
        - a[13] * a[7] * a[10];
    inv[4] = -a[4] * a[10] * a[15] + a[4] * a[11] * a[14] + a[8] * a[6] * a[15]
        - a[8] * a[7] * a[14]
        - a[12] * a[6] * a[11]
        + a[12] * a[7] * a[10];
    inv[8] = a[4] * a[9] * a[15] - a[4] * a[11] * a[13] - a[8] * a[5] * a[15]
        + a[8] * a[7] * a[13]
        + a[12] * a[5] * a[11]
        - a[12] * a[7] * a[9];
    inv[12] = -a[4] * a[9] * a[14] + a[4] * a[10] * a[13] + a[8] * a[5] * a[14]
        - a[8] * a[6] * a[13]
        - a[12] * a[5] * a[10]
        + a[12] * a[6] * a[9];
    inv[1] = -a[1] * a[10] * a[15] + a[1] * a[11] * a[14] + a[9] * a[2] * a[15]
        - a[9] * a[3] * a[14]
        - a[13] * a[2] * a[11]
        + a[13] * a[3] * a[10];
    inv[5] = a[0] * a[10] * a[15] - a[0] * a[11] * a[14] - a[8] * a[2] * a[15]
        + a[8] * a[3] * a[14]
        + a[12] * a[2] * a[11]
        - a[12] * a[3] * a[10];
    inv[9] = -a[0] * a[9] * a[15] + a[0] * a[11] * a[13] + a[8] * a[1] * a[15]
        - a[8] * a[3] * a[13]
        - a[12] * a[1] * a[11]
        + a[12] * a[3] * a[9];
    inv[13] = a[0] * a[9] * a[14] - a[0] * a[10] * a[13] - a[8] * a[1] * a[14]
        + a[8] * a[2] * a[13]
        + a[12] * a[1] * a[10]
        - a[12] * a[2] * a[9];
    inv[2] = a[1] * a[6] * a[15] - a[1] * a[7] * a[14] - a[5] * a[2] * a[15]
        + a[5] * a[3] * a[14]
        + a[13] * a[2] * a[7]
        - a[13] * a[3] * a[6];
    inv[6] = -a[0] * a[6] * a[15] + a[0] * a[7] * a[14] + a[4] * a[2] * a[15]
        - a[4] * a[3] * a[14]
        - a[12] * a[2] * a[7]
        + a[12] * a[3] * a[6];
    inv[10] = a[0] * a[5] * a[15] - a[0] * a[7] * a[13] - a[4] * a[1] * a[15]
        + a[4] * a[3] * a[13]
        + a[12] * a[1] * a[7]
        - a[12] * a[3] * a[5];
    inv[14] = -a[0] * a[5] * a[14] + a[0] * a[6] * a[13] + a[4] * a[1] * a[14]
        - a[4] * a[2] * a[13]
        - a[12] * a[1] * a[6]
        + a[12] * a[2] * a[5];
    inv[3] = -a[1] * a[6] * a[11] + a[1] * a[7] * a[10] + a[5] * a[2] * a[11]
        - a[5] * a[3] * a[10]
        - a[9] * a[2] * a[7]
        + a[9] * a[3] * a[6];
    inv[7] = a[0] * a[6] * a[11] - a[0] * a[7] * a[10] - a[4] * a[2] * a[11]
        + a[4] * a[3] * a[10]
        + a[8] * a[2] * a[7]
        - a[8] * a[3] * a[6];
    inv[11] = -a[0] * a[5] * a[11] + a[0] * a[7] * a[9] + a[4] * a[1] * a[11]
        - a[4] * a[3] * a[9]
        - a[8] * a[1] * a[7]
        + a[8] * a[3] * a[5];
    inv[15] = a[0] * a[5] * a[10] - a[0] * a[6] * a[9] - a[4] * a[1] * a[10]
        + a[4] * a[2] * a[9]
        + a[8] * a[1] * a[6]
        - a[8] * a[2] * a[5];

    let det = a[0] * inv[0] + a[1] * inv[4] + a[2] * inv[8] + a[3] * inv[12];
    if det.abs() < 1e-18 {
        let mut i = [[0.0; 4]; 4];
        for (k, col) in i.iter_mut().enumerate() {
            col[k] = 1.0;
        }
        return i;
    }

    let d = 1.0 / det;
    let mut o = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            o[c][r] = inv[c * 4 + r] * d;
        }
    }
    o
}

pub fn to_f32(m: Mat4) -> [[f32; 4]; 4] {
    let mut o = [[0.0f32; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            o[c][r] = m[c][r] as f32;
        }
    }
    o
}

/// `a * b`, ambas column-major.
pub fn mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut o = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            o[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    o
}

/// Aplica a matriz a um ponto (w = 1), devolvendo coordenada homogenea.
///
/// Na CPU isto serve para picking (desprojetar um pixel de volta ao mundo) e para os
/// testes de frustum. No caminho de desenho quem transforma vertices e a GPU.
pub fn transform(m: Mat4, v: [f64; 3]) -> [f64; 4] {
    let mut o = [0.0; 4];
    for r in 0..4 {
        o[r] = m[0][r] * v[0] + m[1][r] * v[1] + m[2][r] * v[2] + m[3][r];
    }
    o
}

fn look_at(olho: [f64; 3], alvo: [f64; 3], up: [f64; 3]) -> Mat4 {
    let f = norm(sub(alvo, olho)); // frente
    let s = norm(cross(f, up)); // direita
    let u = cross(s, f); // cima real

    let mut m = [[0.0; 4]; 4];
    m[0] = [s[0], u[0], -f[0], 0.0];
    m[1] = [s[1], u[1], -f[1], 0.0];
    m[2] = [s[2], u[2], -f[2], 0.0];
    m[3] = [-dot(s, olho), -dot(u, olho), dot(f, olho), 1.0];
    m
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(v: [f64; 3]) -> [f64; 3] {
    let l = dot(v, v).sqrt();
    if l < 1e-12 {
        [0.0, 0.0, -1.0]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam() -> OrbitCamera {
        OrbitCamera::enquadrando([0.0, 0.0, 0.0], 8_000.0)
    }

    #[test]
    fn identidade_multiplicada_nao_muda_nada() {
        let mut i = [[0.0; 4]; 4];
        for (k, coluna) in i.iter_mut().enumerate() {
            coluna[k] = 1.0;
        }
        let m = cam().view();
        let r = mul(i, m);
        for c in 0..4 {
            for l in 0..4 {
                assert!((r[c][l] - m[c][l]).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn a_camera_orbita_mantendo_a_distancia_do_alvo() {
        let mut c = cam();
        let d0 = c.distancia;
        for _ in 0..12 {
            c.orbitar(0.5, 0.1);
            let p = c.posicao();
            let d = dot(sub(p, c.alvo), sub(p, c.alvo)).sqrt();
            assert!((d - d0).abs() < 1e-6, "distancia mudou: {d} != {d0}");
        }
    }

    #[test]
    fn pitch_nao_passa_do_polo() {
        let mut c = cam();
        for _ in 0..100 {
            c.orbitar(0.0, 1.0);
        }
        use std::f64::consts::FRAC_PI_2;
        assert!(c.pitch < FRAC_PI_2, "pitch travou em {}", c.pitch);
        for _ in 0..300 {
            c.orbitar(0.0, -1.0);
        }
        assert!(c.pitch > -FRAC_PI_2);
        // A view matrix continua finita nos extremos.
        assert!(c.view().iter().flatten().all(|v| v.is_finite()));
    }

    #[test]
    fn zoom_respeita_os_limites() {
        let mut c = cam();
        for _ in 0..200 {
            c.zoom(0.5);
        }
        assert!(c.distancia >= c.near * 4.0 - 1e-9, "{}", c.distancia);
        for _ in 0..400 {
            c.zoom(2.0);
        }
        assert!(c.distancia <= c.far * 0.5 + 1e-9, "{}", c.distancia);
    }

    #[test]
    fn o_alvo_cai_no_centro_da_tela() {
        let c = cam();
        let clip = transform(c.view_proj(16.0 / 9.0), c.alvo);
        assert!(clip[3] > 0.0, "alvo atras da camera: w={}", clip[3]);
        let ndc = [clip[0] / clip[3], clip[1] / clip[3]];
        assert!(ndc[0].abs() < 1e-9 && ndc[1].abs() < 1e-9, "ndc = {ndc:?}");
    }

    #[test]
    fn profundidade_vai_de_zero_no_near_a_um_no_far() {
        let c = cam();
        let vp = c.view_proj(1.0);
        let dir = norm(sub(c.alvo, c.posicao()));
        let olho = c.posicao();

        let ponto_a = |d: f64| {
            [
                olho[0] + dir[0] * d,
                olho[1] + dir[1] * d,
                olho[2] + dir[2] * d,
            ]
        };

        let perto = transform(vp, ponto_a(c.near));
        let longe = transform(vp, ponto_a(c.far));

        let z_perto = perto[2] / perto[3];
        let z_longe = longe[2] / longe[3];

        // Convencao wgpu: near -> 0, far -> 1. Se isto inverter, o depth test descarta
        // a cena inteira e a tela fica vazia.
        assert!(z_perto.abs() < 1e-6, "z no near = {z_perto}");
        assert!((z_longe - 1.0).abs() < 1e-6, "z no far = {z_longe}");
    }

    #[test]
    fn enquadramento_cobre_a_extensao_da_cena() {
        // Os quatro cantos de uma area de 8 km tem que cair dentro do frustum.
        let c = OrbitCamera::enquadrando([0.0, 0.0, 0.0], 8_000.0);
        let vp = c.view_proj(16.0 / 9.0);
        let meio = 4_000.0;
        for &(x, z) in &[(-meio, -meio), (meio, -meio), (-meio, meio), (meio, meio)] {
            let clip = transform(vp, [x, 0.0, z]);
            assert!(clip[3] > 0.0, "canto ({x},{z}) atras da camera");
            let ndc = [clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]];
            assert!(
                ndc[0].abs() <= 1.0 && ndc[1].abs() <= 1.0,
                "canto ({x},{z}) fora da tela: {ndc:?}"
            );
            assert!(
                (0.0..=1.0).contains(&ndc[2]),
                "canto fora do range de z: {ndc:?}"
            );
        }
    }

    #[test]
    fn a_inversa_desfaz_a_view_proj() {
        let c = cam();
        let vp = c.view_proj(16.0 / 9.0);
        let inv = inverse(vp);

        // Um ponto do mundo -> clip -> NDC -> de volta ao mundo tem que voltar igual.
        for p in [
            [0.0, 0.0, 0.0],
            [1_000.0, 50.0, -2_000.0],
            [-3_500.0, 0.0, 1_200.0],
        ] {
            let clip = transform(vp, p);
            let ndc = [clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]];
            let volta = transform(inv, ndc);
            let w = volta[3];
            assert!(w.abs() > 1e-12, "w degenerado");
            for k in 0..3 {
                assert!(
                    (volta[k] / w - p[k]).abs() < 1e-3,
                    "eixo {k}: {} != {}",
                    volta[k] / w,
                    p[k]
                );
            }
        }
    }

    #[test]
    fn inversa_de_matriz_singular_devolve_identidade_em_vez_de_nan() {
        // Sem esta guarda, uma view_proj degenerada espalharia NaN pelo shader de ceu.
        let zero = [[0.0; 4]; 4];
        let i = inverse(zero);
        assert!(i.iter().flatten().all(|v| v.is_finite()));
        for (k, coluna) in i.iter().enumerate() {
            assert_eq!(coluna[k], 1.0, "a diagonal deveria ser 1");
        }
    }

    #[test]
    fn aspecto_zero_nao_gera_nan() {
        let c = cam();
        assert!(c.view_proj(0.0).iter().flatten().all(|v| v.is_finite()));
    }
}
