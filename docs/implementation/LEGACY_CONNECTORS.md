# Conectores herdados: isolamento e proibição de dependência

O código histórico contém conectores para Esri, OSM tiles, NASA GIBS, SIGSC,
Mapbox, Google 3D e Poly Haven. Eles não foram tratados como core.

## Browser

`app/ambiente.js` mantém os adaptadores visuais por compatibilidade, mas todos
estão atrás de uma allowlist de fontes remotas e exigem
`network_mode=import_assisted`. O boot usa `naturalearth_local`. Ao abrir um
projeto em modo local com fonte remota salva, o ambiente retorna à base local.

O token Mapbox é somente `sessionStorage`; a migração remove qualquer
`token_mapbox` legado de `projeto.json`.

## Servidor

Conectores de Poly Haven passam por `NetworkPolicy`, cuja configuração padrão é
`offline_strict`. A rota `/dem` não baixa mais tiles automaticamente: ela serve
somente dados já materializados. Um importador remoto futuro precisa publicar
pacote imutável com licença/hash/proveniência antes do uso.

## Regra para a próxima IA

Não mova URLs de providers para módulos core e não use erro de rede como motivo
para retornar geometria, imagem, altura ou inferência fictícia. Quando um
conector for necessário:

```text
autorização explícita
→ download em processo isolado
→ validação de formato/licença/checksum
→ pacote local imutável
→ registro de source_id/hash
→ uso offline pelo ARCZ
```

Apenas visualização temporária, claramente marcada, pode consumir um provider
em `import_assisted`; ela não pode ser requisito para reabrir o projeto.
