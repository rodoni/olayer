# SDK TypeScript (Web)
## Componentes da SDK TypeScript (C4 Model - Nível 3)

Este documento apresenta a organização de alto nível dos componentes do **Olayer TS SDK** (localizado em `sdk/ts`), servindo como mapa de navegação para a documentação de arquitetura específica de cada submódulo.

---

## 1. Diagrama de Componentes da SDK TS

O diagrama abaixo detalha a estrutura interna da SDK TypeScript e suas interações com a ponte WebAssembly, a aplicação Host e as APIs gráficas do navegador.

```mermaid
graph TB
    classDef host fill:#1168BD,stroke:#0f5ca7,color:#ffffff,stroke-width:2px;
    classDef jsComponent fill:#FFF9C4,stroke:#FBC02D,color:#5D4037,stroke-width:2px;
    classDef wasmBridge fill:#FFE082,stroke:#FFB300,color:#5D4037,stroke-width:2px;
    classDef browser fill:#E1F5FE,stroke:#0288D1,color:#01579B,stroke-width:2px;

    %% Host App
    host["📱 Host App Web<br>[TypeScript/React/Vue]"]:::host

    subgraph TS_SDK_Boundary ["Olayer TS SDK (sdk/ts)"]
        %% Componentes Principais
        controller["🎮 TS Controller<br>[Component]<br>Gerencia o loop de animação, eventos de câmera e controle de FPS."]:::jsComponent
        layer_manager["🥞 Layer Manager<br>[Component]<br>Gerencia a pilha de camadas (Layer Stack) e segrega a repintura."]:::jsComponent
        map_data_stack["📥 TS Map Data Stack<br>[Component]<br>Gerencia a infraestrutura de dados de mapa, fontes de dados e caches."]:::jsComponent
        
        %% Pipeline Gráfico
        gpu_pipeline["🎨 GPU Render Pipeline<br>[Component]<br>Renderização de malhas de terreno e mapas estáticos (WebGL2)."]:::jsComponent
        cpu_pipeline["🎯 CPU/Target Pipeline<br>[Component]<br>Projeção de alvos, anti-cluttering e desenho de etiquetas."]:::jsComponent
        atlas_manager["🖼️ Texture Atlas Manager<br>[Component]<br>Compila e compacta símbolos (SVG, PNG, procedurais) na GPU."]:::jsComponent
    end

    %% Elementos de Borda
    wasm_bridge["🔗 Bridge WASM (wasm-bindgen)<br>[WASM Interop]"]:::wasmBridge
    canvas_2d["🖥️ Canvas 2D API<br>[Browser API]"]:::browser
    webgl_ctx["🎮 WebGL2 / WebGPU Context<br>[Browser API]"]:::browser

    %% Fluxos de Entrada e Saída
    host -->|1. Configura e interage| controller
    host -->|2. Envia pings de radar| controller
    controller -->|Registra alvos| wasm_bridge
    map_data_stack -->|3. Injeta binários de mapa e relevo| wasm_bridge

    %% Fluxos Internos da SDK
    controller -->|Coordenador do ciclo| layer_manager
    layer_manager -->|Pinta fundo e terreno| gpu_pipeline
    layer_manager -->|Pinta alvos e etiquetas| cpu_pipeline
    
    gpu_pipeline -->|Consulta matrizes e terreno| wasm_bridge
    cpu_pipeline -->|Consulta posições interpoladas| wasm_bridge
    cpu_pipeline -->|Requer coordenadas UV| atlas_manager

    %% Renderização física
    gpu_pipeline -->|Desenha no buffer| webgl_ctx
    cpu_pipeline -->|Desenha no buffer| canvas_2d
    atlas_manager -->|Gera e atualiza textura| webgl_ctx

    linkStyle 0,1,2,3,4,5,6,7,8,9,10,11,12 stroke:#555,stroke-width:1.5px;
```

---

## 2. Detalhamento e Arquitetura dos Submódulos

Cada componente principal da SDK está documentado em um arquivo de arquitetura detalhado (`arch.md`) localizado em seu respectivo diretório de especificação técnica:

### 🎮 2.1 TS Controller
Ponto de entrada unificado da SDK. Atua como o maestro do ciclo de vida, loop principal e throttling dinâmico de FPS.
* Detalhamento técnico completo: [arch.md](file:///c:/Users/rafae/projects/rust/olayer/docs/sdk/ts/controller/arch.md)

### 🥞 2.2 Layer Manager
Coordenador da pilha de camadas (Layer Stack), responsável pela ordenação e pela otimização de renderização segregada entre elementos dinâmicos e estáticos.
* Detalhamento técnico completo: [arch.md](file:///c:/Users/rafae/projects/rust/olayer/docs/sdk/ts/layers/arch.md)

### 📥 2.3 Map Data Stack (Providers)
Módulo encarregado do carregamento sob demanda, paginação e cacheamento inteligente (com política LRU) de dados cartográficos (MVT, WMTS) e de terreno (DTED).
* Detalhamento técnico completo: [arch.md](file:///c:/Users/rafae/projects/rust/olayer/docs/sdk/ts/providers/arch.md)

### 🎨 2.4 Render Pipelines & Texture Atlas
Motores gráficos de desenho. Contém o pipeline de renderização GPU (WebGL2), o pipeline de radar CPU (com algoritmo de prevenção de sobreposição de etiquetas/anti-cluttering) e o Texture Atlas Manager.
* Detalhamento técnico completo: [arch.md](file:///c:/Users/rafae/projects/rust/olayer/docs/sdk/ts/renderer/arch.md)
