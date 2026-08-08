//! Escolha determinística de espécie e porte para cada árvore.
//!
//! O gerador de malha desenha um cone genérico. Este módulo diz **qual** árvore
//! cada instância é, para que o app substitua o cone por um modelo real — os
//! 61 GLB do Kenney Nature Kit (CC0) copiados para `assets/vegetacao`.
//!
//! ## Por que a escolha vive aqui, e não no app
//!
//! Ela precisa ser determinística e reproduzível: reabrir o projeto não pode
//! trocar as espécies, senão um enquadramento de câmera salvo deixa de valer.
//! A semente sai da posição e do id da superfície, exatamente como a semeadura
//! — assim a mesma árvore recebe sempre a mesma espécie, sem guardar nada.
//!
//! ## Licença
//!
//! Kenney Nature Kit é **CC0** (domínio público). Não exige atribuição nem
//! impõe share-alike, o que o torna seguro para uso comercial. O original
//! permanece no banco-mestre; `assets/vegetacao` é cópia de trabalho.

/// Como a árvore se parece. O nome é o prefixo do arquivo no kit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Especie {
    /// Copa arredondada e densa. A mais comum em rua de bairro.
    Comum,
    /// Copa cônica, tipo araucária ou pinheiro.
    Conica,
    /// Copa larga e baixa, de praça.
    Larga,
    /// Palmeira — dominante no litoral catarinense.
    Palmeira,
    /// Arbusto alto, para bordas e canteiros.
    Arbusto,
}

impl Especie {
    /// Nome-base do arquivo GLB dentro de `assets/vegetacao/kenney-nature-kit`.
    ///
    /// Sem extensão nem variação de estação: o app escolhe entre `.glb`,
    /// `_dark.glb` e `_fall.glb` conforme a paleta da cena.
    pub fn arquivo(self) -> &'static str {
        match self {
            Self::Comum => "tree_default",
            Self::Conica => "tree_cone",
            Self::Larga => "tree_oak",
            Self::Palmeira => "tree_palm",
            Self::Arbusto => "tree_thin",
        }
    }

    /// Altura típica em metros, como faixa `(mínima, máxima)`.
    ///
    /// Palmeira é alta e estreita; arbusto não passa de altura de muro. Sortear
    /// tudo na mesma faixa produz um bosque uniforme que não parece bairro.
    pub fn faixa_altura_m(self) -> (f64, f64) {
        match self {
            Self::Comum => (5.0, 11.0),
            Self::Conica => (7.0, 16.0),
            Self::Larga => (6.0, 13.0),
            Self::Palmeira => (6.0, 14.0),
            Self::Arbusto => (1.5, 3.0),
        }
    }

    /// Razão entre o raio da copa e a altura.
    pub fn esbeltez(self) -> f64 {
        match self {
            Self::Comum => 0.30,
            Self::Conica => 0.22,
            Self::Larga => 0.42,
            // Palmeira tem copa pequena em relação ao tronco.
            Self::Palmeira => 0.18,
            Self::Arbusto => 0.55,
        }
    }
}

/// Mistura de espécies de um lugar.
///
/// Os pesos são a proporção esperada de cada espécie. Não somam 1 por
/// obrigação — a escolha normaliza.
#[derive(Debug, Clone, Copy)]
pub struct Bioma {
    pub comum: f64,
    pub conica: f64,
    pub larga: f64,
    pub palmeira: f64,
    pub arbusto: f64,
}

impl Bioma {
    /// Litoral de Santa Catarina: palmeira presente, conífera rara.
    ///
    /// Não é levantamento botânico — é a proporção que faz o bairro parecer o
    /// bairro na vista aérea. Trocar por dados reais de arborização urbana é
    /// substituir esta constante, não reescrever o gerador.
    pub const LITORAL_SC: Self = Self {
        comum: 0.42,
        conica: 0.06,
        larga: 0.24,
        palmeira: 0.20,
        arbusto: 0.08,
    };

    /// Mata atlântica de encosta: sem palmeira ornamental, copa densa.
    pub const MATA_ATLANTICA: Self = Self {
        comum: 0.50,
        conica: 0.10,
        larga: 0.34,
        palmeira: 0.02,
        arbusto: 0.04,
    };

    fn pesos(&self) -> [(Especie, f64); 5] {
        [
            (Especie::Comum, self.comum),
            (Especie::Conica, self.conica),
            (Especie::Larga, self.larga),
            (Especie::Palmeira, self.palmeira),
            (Especie::Arbusto, self.arbusto),
        ]
    }

    /// Espécie sorteada por `r ∈ [0, 1)`.
    ///
    /// Roleta simples sobre os pesos normalizados. Peso zero nunca é sorteado,
    /// e um bioma com todos os pesos zerados cai em `Comum` em vez de entrar em
    /// divisão por zero.
    pub fn especie(&self, r: f64) -> Especie {
        let pesos = self.pesos();
        let total: f64 = pesos.iter().map(|(_, p)| p.max(0.0)).sum();
        if total <= 0.0 {
            return Especie::Comum;
        }
        let mut alvo = r.clamp(0.0, 1.0 - 1e-12) * total;
        for (e, p) in pesos {
            let p = p.max(0.0);
            if alvo < p {
                return e;
            }
            alvo -= p;
        }
        Especie::Comum
    }
}

/// Uma árvore já resolvida: espécie, porte e giro.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instancia {
    pub especie: Especie,
    pub altura_m: f64,
    pub raio_copa_m: f64,
    pub giro_rad: f64,
}

/// Resolve a instância a partir da posição, de forma determinística.
///
/// A mesma coordenada devolve sempre a mesma árvore — sem guardar nada. É o que
/// permite regenerar o entorno e reencontrar a floresta idêntica.
pub fn resolver(bioma: Bioma, leste: f64, norte: f64, semente: u64) -> Instancia {
    // Quantiza em centímetros antes de hashear: `f64` de posições calculadas
    // varia no último bit entre execuções, e isso trocaria a espécie.
    let x = (leste * 100.0).round() as i64;
    let y = (norte * 100.0).round() as i64;
    let base = mistura(x as u64 ^ (y as u64).rotate_left(32) ^ semente);

    let r1 = fracao(base);
    let r2 = fracao(mistura(base ^ 0x9E37_79B9));
    let r3 = fracao(mistura(base ^ 0x85EB_CA6B));

    let especie = bioma.especie(r1);
    let (lo, hi) = especie.faixa_altura_m();
    let altura_m = lo + (hi - lo) * r2;

    Instancia {
        especie,
        altura_m,
        raio_copa_m: altura_m * especie.esbeltez(),
        giro_rad: r3 * std::f64::consts::TAU,
    }
}

/// splitmix64: espalha os bits do inteiro de entrada.
fn mistura(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn fracao(z: u64) -> f64 {
    (z >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mesma_posicao_devolve_sempre_a_mesma_arvore() {
        // Reabrir o projeto nao pode trocar as especies: um enquadramento de
        // camera salvo deixaria de valer.
        let a = resolver(Bioma::LITORAL_SC, 12.5, -30.25, 7);
        let b = resolver(Bioma::LITORAL_SC, 12.5, -30.25, 7);
        assert_eq!(a, b);
    }

    #[test]
    fn posicoes_diferentes_dao_arvores_diferentes() {
        // Sem isto a rua inteira teria a mesma arvore clonada.
        let mut vistas = std::collections::HashSet::new();
        for i in 0..40 {
            let inst = resolver(Bioma::LITORAL_SC, i as f64 * 7.3, i as f64 * -4.1, 1);
            vistas.insert((inst.especie, (inst.altura_m * 100.0) as i64));
        }
        assert!(vistas.len() > 20, "so {} variacoes em 40 arvores", vistas.len());
    }

    #[test]
    fn o_ruido_do_ultimo_bit_nao_troca_a_especie() {
        // Posicoes calculadas variam no ultimo bit entre execucoes. Sem a
        // quantizacao em centimetros, a especie mudaria sozinha.
        let a = resolver(Bioma::LITORAL_SC, 10.0, 20.0, 3);
        let b = resolver(Bioma::LITORAL_SC, 10.0 + 1e-12, 20.0 - 1e-12, 3);
        assert_eq!(a, b);
    }

    #[test]
    fn o_bioma_respeita_as_proporcoes() {
        // Litoral SC tem 20% de palmeira; a amostra grande tem de chegar perto.
        let mut palmeiras = 0;
        const N: usize = 4000;
        for i in 0..N {
            let inst = resolver(Bioma::LITORAL_SC, i as f64 * 3.7, i as f64 * 2.1, 11);
            if inst.especie == Especie::Palmeira {
                palmeiras += 1;
            }
        }
        let frac = palmeiras as f64 / N as f64;
        assert!(
            (frac - 0.20).abs() < 0.04,
            "palmeiras em {:.1}%, esperava ~20%",
            frac * 100.0
        );
    }

    #[test]
    fn a_mata_atlantica_quase_nao_tem_palmeira() {
        let mut palmeiras = 0;
        for i in 0..2000 {
            let inst = resolver(Bioma::MATA_ATLANTICA, i as f64 * 5.1, i as f64 * -3.3, 5);
            if inst.especie == Especie::Palmeira {
                palmeiras += 1;
            }
        }
        assert!(palmeiras < 100, "{palmeiras} palmeiras em 2000 na mata");
    }

    #[test]
    fn a_altura_fica_na_faixa_da_especie() {
        // Palmeira de 2 m ou arbusto de 15 m arruinariam a escala da cena.
        for i in 0..500 {
            let inst = resolver(Bioma::LITORAL_SC, i as f64 * 1.7, i as f64 * 9.4, 2);
            let (lo, hi) = inst.especie.faixa_altura_m();
            assert!(
                inst.altura_m >= lo && inst.altura_m <= hi,
                "{:?} com {:.1} m, faixa {lo}..{hi}",
                inst.especie,
                inst.altura_m
            );
            assert!(inst.raio_copa_m > 0.0 && inst.raio_copa_m < inst.altura_m);
        }
    }

    #[test]
    fn o_giro_cobre_a_volta_inteira() {
        let mut minimo = f64::MAX;
        let mut maximo = f64::MIN;
        for i in 0..500 {
            let g = resolver(Bioma::LITORAL_SC, i as f64 * 2.3, i as f64, 9).giro_rad;
            assert!((0.0..=std::f64::consts::TAU).contains(&g), "giro {g}");
            minimo = minimo.min(g);
            maximo = maximo.max(g);
        }
        assert!(maximo - minimo > 5.0, "giro concentrado numa faixa so");
    }

    #[test]
    fn bioma_sem_peso_nao_divide_por_zero() {
        let vazio = Bioma {
            comum: 0.0,
            conica: 0.0,
            larga: 0.0,
            palmeira: 0.0,
            arbusto: 0.0,
        };
        assert_eq!(vazio.especie(0.5), Especie::Comum);
    }

    #[test]
    fn peso_zero_nunca_e_sorteado() {
        // Um bioma que declara zero palmeiras nao pode produzir palmeira.
        let sem_palmeira = Bioma {
            palmeira: 0.0,
            ..Bioma::LITORAL_SC
        };
        for i in 0..1000 {
            let r = i as f64 / 1000.0;
            assert_ne!(sem_palmeira.especie(r), Especie::Palmeira, "r = {r}");
        }
    }

    #[test]
    fn cada_especie_aponta_para_um_arquivo_do_kit() {
        // Nome errado vira arvore invisivel na cena, sem erro nenhum.
        for e in [
            Especie::Comum,
            Especie::Conica,
            Especie::Larga,
            Especie::Palmeira,
            Especie::Arbusto,
        ] {
            assert!(e.arquivo().starts_with("tree_"), "{:?}", e);
            assert!(!e.arquivo().ends_with(".glb"), "sem extensao: {:?}", e);
        }
    }
}
