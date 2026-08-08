import { createRustGeneratorPlugin } from "./create-rust-generator.js";

const manifest = Object.freeze({
  "tipo": "gerador",
  "id": "arcz.materials.regional",
  "nome": "Materiais regionais",
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
    "triangulos": 1,
    "memoriaMB": 32,
    "texturasMB": 512,
    "drawCalls": 1
  },
  "entrypoint": "/app/plugins/builtin/materials.js",
  "backend_kind": "materials.generate",
  "parameters_schema": "/schemas/generator-parameters.schema.json",
  "minimum_core_version": "0.2.0"
});
const parameters = Object.freeze([])

export default createRustGeneratorPlugin({ manifest, parameters, jobKind: "materials.generate" });
