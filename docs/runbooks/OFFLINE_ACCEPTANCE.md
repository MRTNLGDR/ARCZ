# Gate de aceitação offline

Execute em máquina de teste com dados locais previamente importados.

1. Bloqueie DNS e todo tráfego de saída, mantendo loopback.
2. Remova variáveis/chaves de provedores.
3. Inicie `servidor.py` com `ARCZ_NETWORK_MODE=offline_strict`.
4. Abra projeto existente.
5. Pesquise endereço no índice local.
6. Ative região/bairro sem gerar a cidade inteira.
7. Gere terreno, lotes, vias, casas, prédios e vegetação a partir de pacotes locais.
8. Cancele um job no meio; confirme ausência de primitiva órfã.
9. Salve, mate o processo e reabra; perda máxima de estado persistente: 5 s.
10. Renderize snapshot/sequência suportada e exporte GLB.
11. Desligue todos os modelos de IA; repita geração procedural.
12. Remova conectores; reabra o projeto pelo hash local.
13. Inspecione sockets/processos: nenhum endereço/imagem/geometria saiu da máquina.

O gate falha se qualquer recurso core ficar pendurado, exibir sucesso sem artefato, criar dado não marcado ou exigir endpoint remoto.
