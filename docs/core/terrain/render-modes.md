# Modos de Renderização do Terreno

## Escopo

Os modos de renderização operam sobre uma malha de elevação já carregada pelo `TerrainEngine`. Eles alteram somente a aparência visual; não alteram altitude geodésica, datum vertical ou resolução de `AltitudeMode`.

A renderização de relevo é habilitada somente nos modos de câmera `2.5D` e `3D`. No modo `2D`, `TerrainLayer` e `TerrainContourLayer` não submetem geometria ao WebGL e os controles de relevo da demo ficam desabilitados.

## Modos disponíveis

| Modo | Representação | Implementação |
|---|---|---|
| `hypsometric` | Cor baseada na faixa de elevação (verde para baixas altitudes até ocre/branco no cume) | WebGL e WGPU |
| `hillshade` | Iluminação analítica Phong baseada na normal e direção da luz solar calculada no fragment shader | WebGL e WGPU |
| `slope` | Cor baseada na inclinação local (verde a vermelho escuro para escarpas íngremes) | WebGL e WGPU |
| `hybrid` | Combinação de hypsometric tint e hillshade | WebGL e WGPU |
| `textured` | Imagem raster XYZ sobre a malha com amostragem Web Mercator | WebGL TypeScript |
| `taws` | Alerta de terreno tático / CFIT relativo à altitude da aeronave (vermelho $\le 150$m, amarelo $\le 600$m) | WebGL e WGPU |
| Contours | Isolinhas analíticas procedurais via `fwidth` no shader com anti-aliasing | WebGL e WGPU |

## Atributos da malha

Cada vértice de terreno possui:

```text
position (vec3<f32>)
normal   (vec3<f32>)
elevation (f32)
slope     (f32)
uv        (vec2<f32>, no TypeScript)
```

`normal` é derivada por diferenças centrais locais $(-dz/dx, -dz/dy, 1)$ e normalizada. O vetor da luz solar (`light_dir`) é transmitido via uniform, permitindo modificar azimute e altitude solar em tempo real (60 FPS) sem reconstrução de buffers na CPU.

## Curvas de Nível Procedurais

As curvas de nível podem ser desenhadas no fragment shader via derivadas de tela analíticas:

```glsl
float c = abs(fract(v_elevation / u_contourInterval - 0.5) - 0.5) / max(0.0001, fwidth(v_elevation / u_contourInterval));
float contourAlpha = 1.0 - clamp(c - 0.5, 0.0, 1.0);
```

Essa abordagem opera tanto em WebGL2 quanto em WGSL sem overhead de CPU ou memória de geometria adicional.

## TypeScript

O `TerrainLayer` expõe:

```ts
layer.setRenderMode("taws");
layer.setTawsReferenceAltitude(1200); // metros
layer.setElevationRange(0, 2500);
layer.setHillshade({ azimuthDeg: 315, altitudeDeg: 45 }); // atualiza uniforms sem reconstruir a malha
layer.setContours(true, 100); // ativa isolinhas procedurais no shader a cada 100m
layer.setImageryTemplate("https://tile.openstreetmap.org/{z}/{x}/{y}.png");
layer.setImageryEnabled(true);
```

## Native / WGPU

O `WgpuGpuPipeline` calcula `normal` e `slope` durante `rebuild_terrain_buffers`. `set_terrain_style` atualiza o buffer de uniforms de 32 bytes:

```text
mode: f32,
min_elevation: f32,
max_elevation: f32,
aircraft_altitude: f32,
light_dir: vec3<f32>,
contour_interval: f32
```

## Limitações

- A resolução da malha permanece em `32x32` no Native e configurável até `64x64` no TypeScript.
- Áreas sem amostra são tratadas como elevação zero pelo renderer da demo.
- O draping atual usa um tile XYZ por vez, sem atlas ou quadtree de texturas.
- A textura XYZ depende de CORS e conectividade de rede.

## Próximas extensões

1. Compartilhar paletas externas (texturas 1D LUT) entre WebGL e WGPU.
2. Draping multi-tile integrado ao `MapDataStack` com quadtree/atlas.
3. Malha adaptativa por zoom (CDLOD / GeoMipMapping).
4. Rótulos de altitude nas curvas de nível principais.
