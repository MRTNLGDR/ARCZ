# Contrato de entrada procedural materializada

## Objetivo

Garantir que o worker Rust nunca precise chamar provedor externo e nunca invente dados para satisfazer um job.

## Contêiner

Cada fonte é um diretório com:

```text
package.json
arcz-generator-inputs.json
outros arquivos licenciados...
```

`package.json` segue `schemas/source-package.schema.json`. Todos os arquivos possuem SHA-256 e tamanho. A importação copia o pacote para armazenamento endereçado por conteúdo.

## Entrada procedural

`arcz-generator-inputs.json` segue `schemas/generator-input-package.schema.json`:

```json
{
  "schema_version": 1,
  "coordinate_system": "WGS84",
  "origin_wgs84": null,
  "data": {
    "parcels": [],
    "roads": [],
    "buildings": [],
    "vegetation_zones": []
  },
  "metadata": {}
}
```

Para `ENU_LOCAL`, `origin_wgs84` é obrigatório. O montador reprojeta vetores WGS84 e entradas ENU de outra origem para a origem da Região Ativa. Uma grade de terreno regular em ENU divergente é recusada, pois reprojetá-la sem reamostragem correta corromperia alturas.

## Precedência

```text
entrada explícita e validada do usuário
> pacote local específico
> pacote local genérico
> fallback procedural explicitamente autorizado
```

Conflitos de IDs em pacotes diferentes são erros. Entradas explícitas podem substituir por ID porque representam uma decisão humana consciente.

## Ausência de dados

- `terrain.generate` sem DEM: erro, salvo `allow_flat_terrain_fallback=true` com parâmetros explícitos.
- lotes/vias/prédios sem arrays válidos: erro.
- casas estimadas: somente `allow_estimated_infill=true`, sempre marcadas estimadas.
- vegetação sem máscara/zona: erro.

## Provenance

Cada entidade deve carregar `source`, `source_ref`, `confidence` e `estimated`. O manifest final registra hashes dos pacotes usados.
