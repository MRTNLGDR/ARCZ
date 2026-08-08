import { createRustGeneratorPlugin } from "./create-rust-generator.js";

const manifest = Object.freeze({
  "tipo": "gerador",
  "id": "arcz.vegetation.regional",
  "nome": "Vegetação e biomas",
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
    "triangulos": 2000000,
    "memoriaMB": 400,
    "texturasMB": 280,
    "drawCalls": 180
  },
  "entrypoint": "/app/plugins/builtin/vegetation.js",
  "backend_kind": "vegetation.generate",
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
    "id": "vegetation_density_multiplier",
    "type": "number",
    "min": 0,
    "max": 4,
    "default": 1
  }
])

export default createRustGeneratorPlugin({ manifest, parameters, jobKind: "vegetation.generate" });
