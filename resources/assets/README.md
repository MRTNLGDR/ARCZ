# ARCZ local asset library

Este diretório é a raiz padrão da biblioteca pesada usada pelo runtime iniciado por `ABRIR_ARCZ.cmd`.

Regras:

- assets utilizados pelo runtime devem permanecer sob a raiz do repositório ARCZ;
- nenhum asset ausente é substituído por mock, primitive ou URL remota;
- modelos, texturas, HDRIs e materiais importados devem possuir origem/licença e SHA-256 nos manifestos correspondentes;
- downloads/importações, quando necessários, acontecem apenas na fase explícita `ARCZ_NETWORK_MODE=import_assisted`;
- o runtime normal opera em `offline_strict`.

Arquivos grandes podem ser gerenciados por um mecanismo de armazenamento/versionamento apropriado, mas a árvore materializada consumida pelo ARCZ deve resolver para este repositório e nunca para um caminho externo implícito.
