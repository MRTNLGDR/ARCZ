import { createRustGeneratorPlugin } from "./create-rust-generator.js";

const manifest = Object.freeze({
  "tipo": "gerador",
  "id": "arcz.houses.regional",
  "nome": "Casas regionais",
  "versao": "2.0.0",
  "apiVersion": "2",
  "escalas": [
    "lote",
    "endereco",
    "quarteirao",
    "bairro"
  ],
  "modos": [
    "globo",
    "walk",
    "render"
  ],
  "capacidades": [
    "region.read",
    "scene.stage",
    "scene.commit",
    "budget.reserve",
    "jobs.progress",
    "jobs.create",
    "jobs.subscribe",
    "jobs.wait",
    "jobs.read_manifest",
    "inputs.resolve"
  ],
  "deterministico": true,
  "worker": "rust",
  "custoBase": {
    "triangulos": 1200000,
    "memoriaMB": 320,
    "texturasMB": 220,
    "drawCalls": 450
  },
  "entrypoint": "/app/plugins/builtin/houses.js",
  "backend_kind": "houses.generate",
  "parameters_schema": "/schemas/generator-parameters.schema.json",
  "minimum_core_version": "0.2.0"
});
const parameters = Object.freeze([
  {
    "id": "quality",
    "type": "enum",
    "options": [
      "LEVE",
      "EQUILIBRADO",
      "ALTO",
      "CINEMATICO"
    ],
    "default": "EQUILIBRADO"
  },
  {
    "id": "allow_estimated_infill",
    "type": "boolean",
    "default": false
  }
])

export default createRustGeneratorPlugin({ manifest, parameters, jobKind: "houses.generate" });
