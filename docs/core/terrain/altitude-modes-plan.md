# Plano de Implementação: Modos de Altitude

## Status

The initial implementation described by this plan is complete for the Core, WASM, C-FFI, Native Controller, TypeScript controller, trajectory/airspace layer hooks, and TypeScript demo target flow. Remaining work is to expose the same controls in the native demo and to provide a first-class effective mesh-height provider for `RelativeToMesh`.

## Objetivo

Adicionar suporte consistente aos modos de altitude sem misturar responsabilidades entre geodesia, terreno e renderização.

Modos iniciais:

- `Absolute`
- `ClampToGround`
- `RelativeToGround`
- `RelativeToMesh`

A resolução deve ocorrer antes da projeção, conversão para ECEF ou geração de buffers GPU.

## Arquitetura

### Geodesia

Local proposto:

```text
core/src/geodesy/altitude.rs
```

Responsabilidades:

- Representar referência vertical.
- Diferenciar altura elipsoidal e ortométrica.
- Validar valores de altitude.
- Realizar conversões de unidades e datum.
- Não depender de `TerrainEngine`.

Tipos previstos:

```rust
pub enum VerticalDatum {
    Ellipsoidal,
    Orthometric,
}
```

```rust
pub struct Height {
    pub meters: f64,
    pub datum: VerticalDatum,
}
```

### Terreno

Local proposto:

```text
core/src/terrain/altitude.rs
```

Responsabilidades:

- Resolver altura em relação ao terreno.
- Consultar `TerrainEngine`.
- Tratar terreno desconhecido.
- Implementar `ClampToGround`, `RelativeToGround` e `RelativeToMesh`.

```rust
pub enum AltitudeMode {
    Absolute,
    ClampToGround,
    RelativeToGround,
    RelativeToMesh,
}
```

### Renderização

Renderers e pipelines não devem decidir regras de altitude.

Fluxo obrigatório:

```text
posição geográfica
→ resolver altitude
→ converter para ECEF/projeção
→ gerar geometria
→ renderizar
```

## Fase 1: Modelo geodésico

Criar `core/src/geodesy/altitude.rs`.

Implementar:

- `VerticalDatum`.
- `Height`.
- Validação de valores finitos.
- Conversão de unidades.
- Documentação sobre datum vertical.

Critérios:

- Altitudes negativas válidas.
- `NaN` e infinito rejeitados.
- Nenhuma dependência de terreno.

## Fase 2: Modelo de posicionamento no terreno

Criar `core/src/terrain/altitude.rs`.

Implementar:

```rust
pub enum AltitudeMode {
    Absolute,
    ClampToGround,
    RelativeToGround,
    RelativeToMesh,
}
```

Criar uma função de resolução:

```rust
pub fn resolve_altitude(
    input_height: f64,
    ground_height: Option<f64>,
    mesh_height: Option<f64>,
    mode: AltitudeMode,
    unknown_policy: AltitudeUnknownPolicy,
) -> Result<f64, TerrainError>
```

Regras:

- `Absolute`: retorna `input_height`.
- `ClampToGround`: retorna `ground_height`.
- `RelativeToGround`: retorna `ground_height + input_height`.
- `RelativeToMesh`: retorna `mesh_height + input_height`.

Adicionar política explícita para terreno ausente:

```rust
pub enum AltitudeUnknownPolicy {
    Reject,
    UseAbsolute,
    UseZero,
}
```

O padrão recomendado é `Reject` para operações aeronáuticas.

## Fase 3: Integração com `TerrainEngine`

Atualizar:

```text
core/src/terrain/engine.rs
```

Adicionar métodos para:

- Consultar elevação do terreno.
- Resolver altitude conforme `AltitudeMode`.
- Diferenciar `ground_height` de `mesh_height`.
- Retornar status de terreno desconhecido.

O comportamento atual de `get_elevation` deve permanecer compatível.

## Fase 4: WASM e FFI

Atualizar:

```text
sdk/ts/wasm/src/lib.rs
sdk/native/src/c_ffi_bridge/mod.rs
```

Expor:

- `AltitudeMode`.
- `AltitudeUnknownPolicy`.
- Função de resolução de altitude.
- Status de terreno.

API TypeScript prevista:

```ts
type AltitudeMode =
  | "absolute"
  | "clamp-to-ground"
  | "relative-to-ground"
  | "relative-to-mesh";
```

O WASM deve converter strings para enums Rust e retornar erros claros para valores inválidos.

## Fase 5: Integração no SDK TypeScript

Criar ou atualizar:

```text
sdk/ts/src/types/altitude.ts
```

Adicionar ao `OlayerController`:

```ts
resolveAltitude(
  latRad: number,
  lonRad: number,
  heightMeters: number,
  mode: AltitudeMode,
): number;
```

Configuração padrão:

```ts
altitudeMode: "absolute"
unknownTerrainPolicy: "reject"
```

O modo `Absolute` deve preservar o comportamento existente.

## Fase 6: Integração nas camadas

Atualizar as camadas que geram posições ou geometrias:

```text
sdk/ts/src/layers/trajectory_ribbon.ts
sdk/ts/src/layers/volumetric_airspace.ts
sdk/ts/src/layers/aeronautical.ts
sdk/ts/src/layers/radar.ts
```

Cada camada deve aceitar:

```ts
altitudeMode?: AltitudeMode;
```

A camada não deve consultar diretamente o `TerrainEngine`; deve delegar ao controller ou a um serviço de resolução.

## Fase 7: Integração com a malha de terreno

Distinguir os valores:

```text
ground elevation
mesh elevation
object elevation
```

O exagero vertical deve afetar somente a representação visual da malha, nunca a altitude geodésica real usada por `RelativeToGround`.

Exemplo:

```text
ground elevation: 800 m
vertical exaggeration: 2x
relative-to-ground offset: 100 m
object altitude: 900 m
mesh visual height: 1600 m
```

`RelativeToMesh` deve utilizar a superfície efetiva da malha, e não simplesmente o DEM original.

## Fase 8: Demo TypeScript

Adicionar controles para:

- Modo de altitude.
- Política de terreno desconhecido.
- Offset vertical.
- Elevação do terreno.
- Altitude resolvida.

Aplicar inicialmente a:

- Aeronaves simuladas.
- Alvos radar.
- Trajetórias.
- Volumes.

## Fase 9: Demo nativa

Adicionar os mesmos controles na UI egui e conectá-los ao `NativeController`.

Validar que:

- `Absolute` mantém a altitude geodésica.
- `ClampToGround` posiciona o objeto na superfície.
- `RelativeToGround` mantém o deslocamento sobre o terreno.
- WASM e Native apresentam resultados equivalentes.

## Fase 10: Testes

### Core

- Cada `AltitudeMode`.
- Cada `AltitudeUnknownPolicy`.
- Altitudes negativas.
- Terreno ausente.
- Diferença entre terreno e mesh.
- Exagero vertical sem alteração da altitude real.

### WASM

- Conversão de strings.
- Erros para modo inválido.
- Resolução com GeoTIFF.
- Paridade com o Core.

### TypeScript

- Configuração padrão absoluta.
- Propagação do modo para camadas.
- Posição relativa ao terreno.
- Fallback para terreno desconhecido.

### Native

- Enum nativo.
- Resolução de altitude.
- Geração correta de vértices.
- Compatibilidade com o controller.

## Fase 11: Documentação

Atualizar:

```text
docs/core/terrain/
docs/core/geodesy/
sdk/ts/README.md
```

Documentar:

- Diferença entre datum vertical e modo de altitude.
- Diferença entre DTM e DSM.
- Efeito do exagero vertical.
- Política para terreno desconhecido.
- Exemplos para aeronaves, rotas e volumes.

## Ordem recomendada

1. Modelo `Height` e `VerticalDatum`.
2. `AltitudeMode` no módulo de terreno.
3. Resolução no `TerrainEngine`.
4. Bindings WASM e FFI.
5. API do controller.
6. Integração nas camadas.
7. Demo TypeScript.
8. Demo Native.
9. Testes de paridade.
10. Documentação final.

## Critérios de aceite

- A geodesia não depende do renderer ou do `TerrainEngine`.
- Os modos relativos ficam no módulo de terreno.
- `Absolute` permanece como comportamento padrão.
- Objetos podem ser fixados ou deslocados em relação ao terreno.
- Terreno desconhecido não é convertido silenciosamente em zero por padrão.
- O exagero vertical não altera a altitude geodésica dos objetos.
- WASM, TypeScript e Native apresentam comportamento equivalente.
