# Pacote mínimo de teste

`minimal-package/` é uma fixture determinística e explicitamente sintética para validar o pipeline técnico. Ela não representa cidade real e não deve ser exibida como dado geográfico.

Copie o diretório para `data/imports/inbox/minimal-package` e importe pelo endpoint local. Use a Região Ativa com origem `[-48.5, -27.15, 0]` e bbox compatível. Após compilar o worker, execute jobs separados para terrain, parcels, roads, houses, buildings e vegetation.
