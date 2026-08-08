# Biblioteca de mobiliario e composicao das plantas

## 1. O acervo existente nao servia

`BANCO DE OPENSORCES.../12_INTERIORES_MOBILIA_E_ITENS_DE_CASA` tem **so** os kits
Kenney (CC0): 340 `.glb` low-poly, estilo cartoon, sem PBR. Serve para prototipo de
jogo, nao para render de arquitetura ao lado do Zenite. Idem `13_NATUREZA` e
`15_EXTERIORES`. Nao ha nenhum movel fotorreal no banco.

## 2. O que foi montado (`crates/arcz-biblioteca`)

66 itens em `biblioteca/`, 270 MB, todos **comercialmente livres**:

| Origem | Itens | Licenca |
|---|---|---|
| Poly Haven (API publica, sem chave) | 52 | CC0 1.0 |
| Gerados pelo ARCZ (`parametrico.rs`) | 14 | proprios |

As 14 pecas geradas existem porque **nao ha equivalente CC0 fotorreal**: cama de
casal e de solteiro, guarda-roupa, bancada de cozinha, geladeira, painel de TV,
tapete, vaso sanitario, bancada com cuba, box de vidro, balcao de recepcao,
espreguicadeira, guarda-sol e churrasqueira. Sao volumes limpos na medida exata da
planta, na mesma paleta neutra (off-white, madeira clara/media, tecido cinza, pedra,
inox, vidro).

Cada pasta tem o modelo, as texturas, `LICENCA.txt`; a raiz tem `manifesto.json`
com SHA-256 de cada arquivo.

```
cargo run -p arcz-biblioteca -- --listar
cargo run -p arcz-biblioteca -- --raiz biblioteca --res 2k
```

Verificacao real (nao e promessa): `ARCZ_BIBLIOTECA=biblioteca cargo test -p
arcz-biblioteca --release --test abre_no_loader_do_arcz -- --ignored biblioteca_real`
abre os 66 no mesmo loader do app.

## 3. Cotas tiradas do proprio modelo (`arcz_model::analise`)

O `zenite.glb` chega achatado (31.198 nodes `Geom-NNNN`), sem hierarquia de
pavimento. A analise soma a area das superficies horizontais por faixa de altura;
os picos sao as lajes:

| Pavimento | Cota no arquivo | Laje |
|---|---|---|
| Terreo | -0,07 m | (lote inteiro) |
| Embasamento | 3,53 m | 22,9 x 28,8 m |
| Tipo 2 | 6,73 m | 21,9 x 30,8 m |
| Tipo 3..6 | 10,03 / 13,23 / 16,53 / 19,73 m | 18,7 x 30,8 m |
| Rooftop | 24,03 m | 14,0 x 16,2 m |

Pe-direito medio 3,29 m. `retangulo_minimo` confirma que a torre esta alinhada aos
eixos do arquivo (giro 0,00 graus).

## 4. Composicao (`crates/arcz-app/src/planta.rs` + `plantas/zenite.json`)

5 plantas (apartamento 2 quartos, recepcao, cafeteria, mercado, rooftop),
44 unidades em 7 pavimentos, **1.785 moveis**.

```
arcz --mobiliar plantas/zenite.json --salvar projetos/zenite-mobiliado.arcz
arcz --modelo modelos/zenite.glb --mobiliar plantas/zenite.json \
     --mobiliar-pavimento Rooftop --png preview/rooftop.png
```

`--salvar` grava os 1.785 sem abrir modelo nenhum. Ja o **viewport** carrega a
geometria e a textura de cada instancia: um pavimento tipo (344 moveis) sobe; o
predio inteiro estoura a memoria. Falta compartilhar malha e textura entre
instancias do mesmo arquivo na GPU — e o proximo passo.

## 5. Bug encontrado no caminho

`gpu.rs` subia a malha do `--modelo` em coordenadas do ARQUIVO com **matriz
identidade**: o predio era desenhado deslocado do lugar onde a cena dizia que ele
estava (no Zenite, 5,6 m a oeste e 24 m ao sul). So aparecia quando algo era
posicionado em relacao ao predio — o mobiliario flutuava ao lado dele. Corrigido
usando a mesma `matriz_modelo` dos demais objetos; ha teste garantindo que o
caminho da CPU (`transformar`) e o da GPU (`matriz_modelo`) dao o mesmo ponto.
