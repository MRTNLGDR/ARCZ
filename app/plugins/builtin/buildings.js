import { createRustGeneratorPlugin } from "./create-rust-generator.js";

const manifest = Object.freeze({
  "tipo": "gerador",
  "id": "arcz.buildings.regional",
  "nome": "Prédios regionais",
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
    "triangulos": 1800000,
    "memoriaMB": 480,
    "texturasMB": 300,
    "drawCalls": 550
  },
  "entrypoint": "/app/plugins/builtin/buildings.js",
  "backend_kind": "buildings.generate",
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
  }
])

export default createRustGeneratorPlugin({ manifest, parameters, jobKind: "buildings.generate" });
