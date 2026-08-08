# Globo cinematográfico sem perder navegação

## Princípio

A abertura é uma apresentação sobre o Cesium real. Não usa vídeo, imagem de fundo ou globo falso.

## Camadas

- sky box/universo e estrelas;
- sky atmosphere e ground atmosphere;
- Sol e Lua;
- fog e water effect;
- nuvens do módulo `ambiente.js`;
- hue, saturation e brightness limitados;
- horizon glow/grain de apresentação sem interceptar input;
- trajetória espaço → órbita → Região Ativa.

## Câmera

`Cesium.Camera.flyTo()` é callback-based. `flyToCamera()` resolve apenas em `complete`, rejeita/cancela em `cancel` e suporta AbortSignal. Assim as etapas não se sobrepõem.

O fluxo:

1. salva estado dos controles;
2. aplica baseline visual;
3. posiciona no espaço;
4. voa para órbita;
5. orbita por heading limitado;
6. voa ao destino, quando válido;
7. restaura navegação;
8. persiste câmera real.

## Segurança e UX

- destino inválido não move câmera;
- botão pular e cancelamento;
- reduced motion pula coreografia;
- falha mostra aviso sem matar render loop;
- controles são restaurados em `finally`;
- abertura roda uma vez por sessão/configuração;
- novo projeto sem região abre visão planetária neutra;
- seleção posterior pode executar aproximação ao sítio.

## Configuração persistida

Duração, altitudes, atmosfera, clouds, stars, sun, moon, fog/density, hue/saturation/brightness, orbit heading e reduced motion são migrados com defaults compatíveis.

## Gates

- visual Cesium local;
- atmosfera/nuvens em GPU alvo;
- cancelamento durante cada etapa;
- navegação depois da abertura;
- FPS/VRAM em perfis leve/equilibrado/alto/cinemático.
