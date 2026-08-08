# Dependências locais obrigatórias e gates de vendor

## Regra

O ARCZ não usa CDN no runtime. Dependência de navegador, peso de IA, wheel
Python e crate Rust precisam estar no disco, com versão, licença e SHA-256.
Ausência de vendor é `BLOCKED`, não sucesso parcial e não autorização para
inserir um CDN temporário.

## CesiumJS 1.143

`index.html` exige os seguintes caminhos locais:

```text
vendor/cesium/Cesium/Cesium.js
vendor/cesium/Cesium/Widgets/widgets.css
vendor/cesium/Cesium/Assets/Textures/NaturalEarthII/tilemapresource.xml
vendor/cesium/Cesium/Workers/
vendor/cesium/Cesium/ThirdParty/
vendor/cesium/LICENSE.md
vendor/cesium/manifest.json
```

O pacote de origem enviado para esta sessão excluía `vendor/`; portanto, a
entrega não finge que o browser está pronto sem esses bytes. Instale uma cópia
local já obtida/licenciada:

```bash
python tools/vendor_cesium.py \
  --source /caminho/local/Build/Cesium \
  --license-file /caminho/local/LICENSE.md \
  --version 1.143.0
```

A ferramenta não acessa a internet, valida a árvore e publica por rename
atômico. Depois execute `python tools/verify_handoff.py`.

## Python

`requirements.txt` define o runtime; `requirements-dev.txt`, os testes. Para
release air-gapped, monte `vendor/python/wheelhouse`, registre hashes/licenças e
instale com:

```bash
python -m pip install --no-index --find-links vendor/python/wheelhouse \
  -r requirements-dev.txt
```

O wheelhouse não foi gerado nesta sessão porque não fazia parte do material
fornecido. Não habilite `pip` remoto no aplicativo.

## Rust

O código exige Rust 1.82+ e crates declarados no workspace. Para build sem rede:

1. em máquina autorizada, execute `cargo vendor vendor/rust/vendor`;
2. audite licenças e hashes;
3. transfira o diretório;
4. copie `.cargo/config.offline.example.toml` para `.cargo/config.toml`;
5. execute `cargo fmt --check`, `cargo check --workspace --all-targets` e
   `cargo test --workspace --all-targets` com a rede bloqueada.

Nenhum `cargo` estava instalado no ambiente desta entrega. O relatório mantém
os três gates Rust como `BLOCKED`.

## Modelos de IA

Pesos não ficam embutidos em código nem são baixados automaticamente. Cada
modelo precisa de manifesto local com licença, checksum, RAM/VRAM, backend e
fallback. Sem modelo, o broker devolve `MODEL_NOT_INSTALLED`; o procedural
continua funcionando sem inferência.
