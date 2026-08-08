import { createRustGeneratorPlugin } from "./create-rust-generator.js";

const manifest = Object.freeze({
  "tipo": "gerador",
  "id": "arcz.roads.regional",
  "nome": "Vias e calçadas",
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
    "triangulos": 300000,
    "memoriaMB": 110,
    "texturasMB": 48,
    "drawCalls": 60
  },
  "entrypoint": "/app/plugins/builtin/roads.js",
  "backend_kind": "roads.generate",
  "parameters_schema": "/schemas/generator-parameters.schema.json",
  "minimum_core_version": "0.2.0"
});
const parameters = Object.freeze([
  {
    "id": "include_sidewalks",
    "type": "boolean",
    "default": true
  }
])

export default createRustGeneratorPlugin({ manifest, parameters, jobKind: "roads.generate" });
