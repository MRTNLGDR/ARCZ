# Shell de modos

A infraestrutura registra Globo, Floorplanner, Render e Walk. Cada modo monta
uma vez, ativa/desativa sem listener residual e descarta recursos no dispose.
Não replique cards/IDs do legado; use `mount-once.js` e panel host.
