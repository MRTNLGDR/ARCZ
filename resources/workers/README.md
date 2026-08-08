# Workers locais opcionais

Instale adaptadores reais aqui como `*.worker.json`. O servidor valida o manifesto e executa o comando sem shell, com `ARCZ_NETWORK_MODE=offline_strict`.

Exemplo de forma — não copie como worker instalado sem possuir o executável:

```json
{
  "schema_version": 1,
  "kind": "render.sequence",
  "command": ["C:/ARCZ/bin/arcz-render.exe", "--request", "{request}", "--output", "{output_dir}"],
  "timeout_seconds": 7200,
  "network_mode": "offline_strict",
  "produces": ["manifest.json", "frames/*.png"]
}
```

Tokens aceitos: `{request}`, `{output_dir}`, `{root}`, `{job_id}`. O executável deve produzir `manifest.json` conforme `generation-manifest.schema.json`, incluindo SHA-256 e tamanho de cada saída. Ausência do executável ou do manifesto é erro; não existe fallback vazio.
