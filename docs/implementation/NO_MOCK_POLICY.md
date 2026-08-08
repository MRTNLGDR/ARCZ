# Política sem mocks, simulações ou sucesso fictício

## O que é proibido no código de produção

- função que retorna resultado fixo apenas para a UI parecer funcional;
- endpoint que devolve `ok: true` sem executar trabalho;
- GLB, PNG, SVG, JSON ou vídeo vazio usado como prova de geração;
- terreno plano silencioso quando DEM não existe;
- casas aleatórias apresentadas como dados reais;
- inferência “estimada” sem confiança, origem e marca `estimated`;
- chamada de API remota escondida como fallback;
- captura de exceção seguida de status concluído;
- timer artificial para simular progresso;
- modelo local ausente substituído por texto ou imagem fabricada;
- banco em memória usado no lugar do banco persistente em produção.

## Fallback permitido

Um fallback é válido somente quando todos os itens forem verdadeiros:

1. está explicitamente habilitado pelo usuário ou perfil;
2. é determinístico;
3. passa pelos mesmos validadores do resultado principal;
4. é marcado `estimated: true`;
5. inclui warning e provenance;
6. não substitui limite cadastral, dado legal ou edição bloqueada;
7. pode ser removido/regenerado sem corromper o projeto.

Exemplo válido: terreno plano solicitado explicitamente para pré-visualização e identificado como estimado. Exemplo inválido: gerar terreno plano porque o DEM não foi encontrado e ocultar essa ausência.

## Erro correto

A ausência de capacidade deve produzir:

```json
{
  "error": {
    "code": "MODEL_NOT_INSTALLED",
    "message": "Modelo local não instalado",
    "retryable": false,
    "details": {},
    "trace_id": "..."
  }
}
```

O código exato varia por subsistema, mas a forma permanece estruturada, auditável e visível.
