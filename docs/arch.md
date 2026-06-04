# Arquitetura de Software: Olayer
## Framework GIS Híbrido para Controle de Tráfego Aéreo (ATC)

Este documento descreve a arquitetura inicial do projeto **Olayer**, mapeada a partir dos requisitos definidos na [Especificação Técnica (spec.md)](file:///c:/Users/rafae/projects/rust/olayer/docs/spec.md). O design utiliza o modelo **C4 Model** (Contexto, Contêineres, Componentes e Processos/Código) para ilustrar a divisão de responsabilidades, fluxos de dados e decisões estruturais de missão crítica.

---

## 1. Nível 1: Diagrama de Contexto do Sistema

O diagrama de contexto descreve como o framework Olayer se posiciona em relação aos atores (desenvolvedores e operadores) e aos sistemas externos da solução ATC.

```mermaid
graph TB
    %% Estilos de nós do C4
    classDef person fill:#08427B,stroke:#073b6e,color:#ffffff,stroke-width:2px;
    classDef system fill:#1168BD,stroke:#0f5ca7,color:#ffffff,stroke-width:2px;
    classDef external fill:#999999,stroke:#888888,color:#ffffff,stroke-width:2px;

    %% Atores
    user["👩‍✈️ Controlador de Tráfego Aéreo<br>[Usuário Final]"]:::person
    dev["💻 Desenvolvedor Host<br>[Engenheiro de Software]"]:::person
    
    %% Sistema Principal (Fronteira Olayer)
    subgraph Olayer_Boundary ["Framework Olayer"]
        olayer["🌍 Olayer GIS ATC Framework<br>[Software System]<br>Framework GIS para processamento e projeção espacial de alta performance."]:::system
    end
    
    %% Sistemas Externos
    host_app["📱 Aplicação Host ATC<br>[Software System]<br>Aplicação cliente ou console ATC (Web/Desktop) que consome o Olayer."]:::system
    geoserver["🗺️ GeoServer / GeoWebCache<br>[External System]<br>Servidor de mapas que fornece MVT, WMTS e estilização SLD."]:::external
    sensor_feed["📡 Feed de Sensores ATC<br>[External System]<br>Provedor de dados brutos (ADS-B, ASTERIX, feeds de radar)."]:::external
    terrain_source["🏔️ Servidor / Repositório de Terreno<br>[External System]<br>Fornece dados de elevação (arquivos DTED) via HTTP ou local."]:::external
    
    %% Relacionamentos
    dev -->|Integra e configura no código| host_app
    user -->|Visualiza alvos e interage com o mapa| host_app
    host_app -->|Delega cálculos GIS e renderização| olayer
    host_app -->|Consome e decodifica dados brutos| sensor_feed
    olayer -->|Consome camadas de mapa e estilos| geoserver
    olayer -->|Consome dados de elevação DTED| terrain_source

    linkStyle 0,1,2,3,4,5 stroke:#555,stroke-width:2px;
```

### Atores e Sistemas

| Elemento | Tipo | Descrição |
| :--- | :--- | :--- |
| **Controlador de Tráfego Aéreo** | Usuário | Operador final que utiliza a tela do radar para monitorar rotas, desvios e alertas de segurança. |
| **Desenvolvedor Host** | Usuário | Desenvolvedor que integra a SDK do Olayer no aplicativo cliente (Web ou Desktop). |
| **Olayer GIS ATC Framework** | Sistema | O escopo do projeto: framework responsável por cálculos geodésicos, projeções, renderização de alvos/terreno e checagens GIS. |
| **Aplicação Host ATC** | Sistema Externo | O software hospedeiro (ex: terminal de controle de aproximação TMA ou centro de rota). Gerencia sockets, regras de negócio e interfaces gerais. |
| **GeoServer / GeoWebCache** | Sistema Externo | Servidor de mapas que centraliza arquivos geográficos (limites de setores, aerovias) e distribui em pedaços otimizados (Tiles). |
| **Feed de Sensores ATC** | Sistema Externo | Infraestrutura de rede que injeta feeds de radar ou ADS-B na aplicação host. O Olayer é agnóstico a esta rede. |
| **Servidor / Repositório de Terreno** | Sistema Externo | Servidor de arquivos ou armazenamento local que fornece os dados de elevação do terreno (DTED) requisitados. |

---

## 2. Nível 2: Diagrama de Contêineres

O Olayer é projetado como um framework híbrido. Ele divide-se em um núcleo compartilhado em Rust e bindings específicos para ambientes web (WebAssembly) e desktop (Nativo).

```mermaid
graph TB
    classDef container fill:#438DD5,stroke:#3b7cbd,color:#ffffff,stroke-width:2px;
    classDef external fill:#999999,stroke:#888888,color:#ffffff,stroke-width:2px;
    classDef host fill:#1168BD,stroke:#0f5ca7,color:#ffffff,stroke-width:2px;

    subgraph Web_Browser ["Ambiente do Navegador (Web Client)"]
        host_web["📱 Host App Web<br>[TypeScript/React/Vue]"]:::host
        ts_sdk["📦 Olayer TS SDK<br>[TypeScript Container]<br>Gerencia a pipeline WebGL, inputs e o loop de renderização."]:::container
        wasm_bind["🔗 WASM Bindings<br>[Rust/JS Bridge]<br>Exportações wasm-bindgen e gerenciamento de buffers de memória."]:::container
        wasm_core["⚙️ Olayer Core (Rust WASM)<br>[WASM Module]<br>Motor lógico compilado para WebAssembly. Geodesia, projeções e indexação DTED."]:::container
    end
    
    subgraph Desktop_OS ["Ambiente Desktop Nativo"]
        host_rust["🖥️ Host App Nativo<br>[Rust / C++]"]:::host
        native_sdk["📦 Olayer Native SDK<br>[Rust Container]<br>Wrapper nativo que expõe APIs locais e pipeline wgpu/Vulkan."]:::container
        rust_core["⚙️ Olayer Core (Nativo)<br>[Rust Library]<br>Compilação nativa do core direto para a arquitetura alvo (x86_64/ARM)."]:::container
        local_disk["💽 Armazenamento DTED Local<br>[File System]<br>Arquivos de terreno DTED no disco local."]:::external
    end
    
    subgraph Map_Server_Stack ["Pilha de Dados de Mapa"]
        geoserver["🗺️ GeoServer + GWC<br>[GeoServer Container]<br>Fornece Vector Tiles (MVT), WMTS e estilos SLD."]:::external
        postgis[("🗄️ PostgreSQL + PostGIS<br>[Database]<br>Armazena feições geográficas espaciais.")]:::external
        terrain_repo["🏔️ Repositório DTED Estático<br>[Data Store]<br>Armazena arquivos binários de elevação de terreno (DTED) via HTTP."]:::external
    end
    
    %% Fluxos Web
    host_web -->|Instancia e inicializa| ts_sdk
    ts_sdk -->|Chama via JS| wasm_bind
    wasm_bind -->|Executa rotinas no core| wasm_core
    ts_sdk -->|Consome MVT/WMTS e SLD via HTTP| geoserver
    ts_sdk -->|Baixa arquivos DTED via HTTP| terrain_repo
    
    %% Fluxos Nativo
    host_rust -->|Importa e inicializa| native_sdk
    native_sdk -->|Chamada direta de função estática| rust_core
    native_sdk -->|Consome MVT/WMTS e SLD via HTTP| geoserver
    native_sdk -->|Lê arquivos DTED do disco| local_disk

    %% Infra de Dados
    geoserver -->|Consulta espacial via SQL| postgis

    linkStyle 0,1,2,3,4,5,6,7,8,9 stroke:#555,stroke-width:2px;
```

### Contêineres do Framework

1. **Olayer Core (Rust - compilável para WASM e Nativo):**
   * **Responsabilidade:** Todo o motor matemático de missão crítica. Não possui acesso a I/O direto para arquivos ou rede na versão WASM (passivo), processando apenas estruturas de memória fornecidas pela camada hospedeira.
   * **Tecnologia:** Rust puro (`f64`).
2. **WASM Bindings (wasm-bindgen):**
   * **Responsabilidade:** Ponte de transição de memória entre a máquina virtual JS e a memória linear do WASM. Minimiza cópias usando referências diretas de buffers (`ArrayBuffer` para DTED/MVT).
   * **Tecnologia:** `wasm-bindgen`, `js-sys`, `web-sys`.
3. **Olayer TS SDK (TypeScript):**
   * **Responsabilidade:** SDK/Framework cliente consumido por aplicações Web. Gerencia o ciclo de vida do elemento visual `<canvas>`, orquestra shaders WebGL/WebGPU e cuida dos cálculos de anti-sobreposição de etiquetas (anti-cluttering) na CPU.
   * **Tecnologia:** TypeScript, WebGL 2.0 / WebGPU, Canvas 2D API.
4. **Olayer Native SDK (Rust):**
   * **Responsabilidade:** Invólucro para aplicações Desktop nativas. Facilita o uso do Core com engines de renderização locais.
   * **Tecnologia:** Rust, opcionalmente bindings C/C++ (`cbindgen`).

---

## 3. Nível 3: Diagrama de Componentes (Internos do Core e SDK)

Este diagrama foca na organização modular interna do **Olayer Core** e do **Olayer TS SDK**, ilustrando como os componentes cooperam para realizar projeções cartográficas e renderizações em tempo real.

```mermaid
graph TB
    classDef component fill:#85B3D1,stroke:#668fa7,color:#000000,stroke-dasharray: 5 5,stroke-width:2px;
    classDef coreComponent fill:#E1F5FE,stroke:#0288D1,color:#01579B,stroke-width:2px;
    classDef wasmBridge fill:#FFF9C4,stroke:#FBC02D,color:#5D4037,stroke-width:2px;
    classDef nativeComponent fill:#C8E6C9,stroke:#388E3C,color:#1B5E20,stroke-width:2px;

    subgraph TS_SDK_Comp ["SDK TypeScript (Web)"]
        ts_controller["🎮 TS Controller<br>Loop (15/60 FPS) & Eventos"]:::component
        ts_layer_manager["🥞 TS Layer Manager<br>Composição e controle de layers"]:::component
        ts_data_manager["📥 TS Data Manager<br>Requisições HTTP MVT/WMTS/DTED"]:::component
        ts_gpu_pipe["🎨 WebGL/WebGPU Pipe<br>Desenho estático base map"]:::component
        ts_cpu_pipe["🎯 WebGL/Canvas 2D Pipe<br>Símbolos (Atlas) & Anti-clutter"]:::component
    end
    
    subgraph WASM_Bridge_Comp ["Interop Web"]
        wasm_bridge["🔗 Bridge WASM (wasm-bindgen)<br>Ponte de memória TS/JS -> Rust"]:::wasmBridge
    end

    subgraph Native_SDK_Comp ["SDK Nativa (Desktop)"]
        native_controller["🎮 Native Controller<br>Loop nativo & Janela (winit)"]:::nativeComponent
        native_layer_manager["🥞 Native Layer Manager<br>Composição e controle de layers nativo"]:::nativeComponent
        native_data_manager["📥 Native Data Manager<br>I/O local e rede (reqwest)"]:::nativeComponent
        native_gpu_pipe["🎨 wgpu Pipe (Matrix)<br>Renderização de terreno/fundo (Vulkan/Metal/DX)"]:::nativeComponent
        native_cpu_pipe["🎯 wgpu Pipe (Vertex)<br>Símbolos (Atlas) & Anti-clutter nativo"]:::nativeComponent
    end

    subgraph FFI_Bridge_Comp ["Interop Nativo"]
        ffi_bridge["🔗 C-FFI Bridge (cbindgen)<br>Exports C-compatible para C++ Host"]:::wasmBridge
    end

    subgraph Rust_Core_Comp ["Módulos do Core Agnóstico (Rust)"]
        geodesy["📐 Geodesy Module<br>Conversões geodésicas ECEF/WGS84"]:::coreComponent
        projections["🗺️ Projections Module<br>LCC, Estereográfica, Web Mercator"]:::coreComponent
        terrain["⛰️ Terrain Engine (DTED)<br>Índice espacial & Altitude O(1)"]:::coreComponent
        sld_parser["📄 SLD Parser<br>Parser XML e estilos de símbolos"]:::coreComponent
        symbol_registry["🎖️ Symbol Registry<br>Registro e resolução de simbologias agnósticas"]:::coreComponent
        interpolator["⏱️ Target Interpolator<br>Dead Reckoning de alvos dinâmicos"]:::coreComponent
    end

    %% Relações SDK TS
    ts_controller --> ts_layer_manager
    ts_layer_manager --> ts_gpu_pipe
    ts_layer_manager --> ts_cpu_pipe
    ts_data_manager --> ts_controller
    ts_data_manager --> wasm_bridge
    ts_gpu_pipe --> wasm_bridge
    ts_cpu_pipe --> wasm_bridge

    %% Relações SDK Nativa
    native_controller --> native_layer_manager
    native_layer_manager --> native_gpu_pipe
    native_layer_manager --> native_cpu_pipe
    native_data_manager --> native_controller
    native_data_manager --> ffi_bridge
    native_gpu_pipe --> ffi_bridge
    native_cpu_pipe --> ffi_bridge
    
    %% Relações Internas da Bridge WASM para o Core
    wasm_bridge --> projections
    wasm_bridge --> terrain
    wasm_bridge --> sld_parser
    wasm_bridge --> symbol_registry
    wasm_bridge --> interpolator

    %% Relações Internas da Bridge FFI para o Core
    ffi_bridge --> projections
    ffi_bridge --> terrain
    ffi_bridge --> sld_parser
    ffi_bridge --> symbol_registry
    ffi_bridge --> interpolator
    
    %% Relações diretas Rust-to-Rust (SDK Nativa para o Core)
    native_gpu_pipe --> projections
    native_gpu_pipe --> terrain
    native_cpu_pipe --> symbol_registry
    native_cpu_pipe --> interpolator

    %% Dependências internas do Rust Core
    projections --> geodesy
    terrain --> geodesy
    interpolator --> geodesy
    symbol_registry --> sld_parser

    linkStyle 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31 stroke:#333,stroke-dasharray: 2 2;
```

### Detalhamento dos Componentes

#### 1. Módulos do Core Rust
* **[Geodesy Module](file:///c:/Users/rafae/projects/rust/olayer/core/src/geodesy):** Fornece as funções matemáticas baseadas no elipsoide de referência WGS84. Realiza transformações bidirecionais entre coordenadas geográficas $(\phi, \lambda, h)$ e cartesianas ECEF $(X, Y, Z)$.
* **[Projections Module](file:///c:/Users/rafae/projects/rust/olayer/core/src/projections):** Contém as fórmulas matemáticas para projetar pontos tridimensionais ou geodésicos em planos 2D. Implementa as projeções Estereográfica, LCC e Mercator.
* **[Terrain Engine (DTED)](file:///c:/Users/rafae/projects/rust/olayer/core/src/terrain):** Gerencia arquivos DTED em memória. Constrói um índice espacial 2D simplificado (Grid) onde cada célula aponta para os bytes de elevação carregados. Permite que consultas de altitude em coordenadas arbitrárias rodem em tempo constante $O(1)$.
* **[SLD Parser](file:///c:/Users/rafae/projects/rust/olayer/core/src/sld):** Analisador sintático (Parser) de XML que converte o padrão OGC SLD (Styled Layer Descriptor) em metadados de estilo estruturados.
* **[Symbol Registry](file:///c:/Users/rafae/projects/rust/olayer/core/src/symbol_registry):** Registro unificado e agnóstico de simbologia que aceita a importação de símbolos customizados nos formatos **SVG** ou **PNG**, além de delegar a decodificação de códigos de símbolos para provedores específicos (como NATO APP-6 ou ICAO civil), retornando primitivas vetoriais estruturadas ou buffers de pixel prontos para o Atlas de Texturas.
* **[Target Interpolator](file:///c:/Users/rafae/projects/rust/olayer/core/src/interpolator):** Mantém a tabela de estado de alvos dinâmicos no espaço geodésico 3D. Para cada alvo, registra o último vetor de estado conhecido. Computa posições interpoladas via Dead Reckoning tridimensional baseada no tempo do sistema (WGS84 LatLon e heading), de forma totalmente desacoplada da projeção de tela.

#### 2. Componentes da SDK TypeScript (Web Client)
* **TS Controller:** Controla o loop de animação da tela no navegador utilizando `requestAnimationFrame` e gerencia a modulação dinâmica de FPS (15 FPS ocioso / 60 FPS ativo).
* **TS Layer Manager:** Coordena a pilha de camadas (Layer Stack) na Web, gerindo o ciclo de pintura otimizado com isolamento de camadas estáticas e dinâmicas.
* **TS Data Manager:** Realiza as chamadas HTTP assíncronas no navegador (`fetch`) para obter os arquivos vetoriais MVT do GeoServer, imagens WMTS, esquemas SLD e binários DTED.
* **WebGL/WebGPU GPU Pipeline:** Vincula buffers de vértices estáticos e renderiza na GPU a partir de matrizes $4 \times 4$ enviadas pela ponte WASM.
* **WebGL/Canvas 2D CPU Pipeline:** Renderiza alvos dinâmicos resolvendo os sprites no *Atlas de Texturas* da GPU e calculando a anti-sobreposição de etiquetas.

#### 3. Componentes da SDK Nativa (Desktop Client)
* **Native Controller:** Controla o loop nativo de frames e gerencia a criação de janelas desktop locais (utilizando a crate `winit` ou o loop de mensagens da aplicação host).
* **Native Layer Manager:** Gerencia a pilha de camadas nativas para controle de visibilidade, mesclagem e repintura em nível nativo.
* **Native Data Manager:** Gerencia a leitura assíncrona de arquivos DTED no disco rígido local e faz requisições HTTP (via `reqwest` ou biblioteca similar) para buscar MVTs/WMTS do GeoServer.
* **wgpu GPU Pipeline:** Compila pipelines e renderiza na GPU (Vulkan, Metal ou DirectX 12) através da biblioteca Rust `wgpu` para desenhar terrenos tridimensionais e mapas de fundo vetoriais.
* **wgpu CPU/Vertex Pipeline:** Renderiza os alvos dinâmicos no desktop usando chamadas instanciadas e *billboards* a partir de um atlas de textura local.

#### 4. Camadas de Interoperabilidade (Bridges)
* **Bridge WASM (wasm-bindgen):** Ponte de transição de memória e FFI que exporta funções do Core Rust para o formato TypeScript/JavaScript no navegador, usando referências diretas de memória.
* **C-FFI Bridge (cbindgen):** Ponte de exportação C-API (`libolayer_native.h`) gerada pelo `cbindgen`, expondo interfaces compatíveis com vinculação direta para hospedeiros em C, C++ ou outras linguagens compiladas.

---

## 4. Nível 4: Código e Fluxos de Processo (Sequence Diagrams)

### 4.1 Ingestão de Pings e Loop de Renderização Dinâmico (FPS Throttling)

Este diagrama detalha como o sistema lida com o recebimento lento de dados de sensores (geralmente 1 Hz) e o renderiza suavemente na tela (15 a 60 FPS) usando *Dead Reckoning*.

```mermaid
sequenceDiagram
    autonumber
    participant Host as Host App (TS / C++ / Rust)
    participant SDK as Olayer SDK (TS / Native)
    participant Core as Olayer Core (WASM / Nativo)
    participant GPU as GPU (WebGL / WebGPU / wgpu)

    Note over Host, Core: 1. Ingestão de Dados de Sensores (Assíncrono ~1 Hz)
    Host->>SDK: updateTarget(id, latitude, longitude, altitude, heading, speed, timestamp)
    SDK->>Core: update_target(TargetState) (Via WASM ou Link Nativo)
    Core->>Core: Salva no registro de estados (Interpolator)

    Note over Host, GPU: 2. Loop de Renderização (Dinâmico: 15 FPS ocioso / 60 FPS ativo)
    Host->>SDK: renderFrame(currentSystemTime, cameraState)
    
    rect rgb(230, 245, 255)
        Note over SDK, Core: Canal de Matrizes (Orientado à GPU - Fundo e Terreno)
        SDK->>Core: get_view_projection_matrix(cameraState)
        Core-->>SDK: Matrix4x4 (LCC / Estereográfica / ECEF)
        SDK->>GPU: Atualiza Uniform / Render Pipeline ('u_viewProjMatrix')
        SDK->>GPU: DrawInstanced / DrawElements (Mapas de Fundo & Elevações)
    end

    rect rgb(255, 245, 230)
        Note over SDK, Core: Canal de Vértices Projetados (Orientado à CPU - Alvos e Símbolos)
        SDK->>Core: interpolate_all(currentSystemTime)
        Core->>Core: Calcula Dead Reckoning em 3D geodésico (elipsoide WGS84)
        Core-->>SDK: Lista de alvos interpolados [id, LatLon, heading_rad]
        SDK->>Core: project(LatLon) (Para cada alvo)
        Core-->>SDK: Coordenadas de Tela (X, Y)
        SDK->>SDK: Resolve símbolos no Atlas de Texturas (ICAO/NATO) e executa Anti-cluttering
        SDK->>GPU: Renderiza símbolos (Billboards e Instanced sprites) e textos
    end
```

### 4.2 Carga de Terreno DTED e Processamento de Alertas Verticais (MSAW)

Este diagrama ilustra o carregamento de arquivos DTED na memória e o cálculo de alertas verticais e perfil de elevação, detalhando a diferença de consumo de dados entre Web e Desktop.

```mermaid
sequenceDiagram
    autonumber
    participant Host as Host App (TS / C++ / Rust)
    participant SDK as Olayer SDK (TS / Native)
    participant Source as Fonte de Terreno (HTTP / Disco)
    participant Core as Olayer Core (WASM / Nativo)

    Note over SDK, Source: Fase 1: Carregamento de Terreno (Sob Demanda)
    alt Para Ambiente Web (TS Client)
        SDK->>Source: HTTP GET (Tile DTED)
        Source-->>SDK: ArrayBuffer (Dados binários)
    else Para Ambiente Nativo (Desktop Client)
        SDK->>Source: Leitura I/O Local (Caminho do arquivo DTED)
        Source-->>SDK: Buffer binário de elevação
    end
    SDK->>Core: load_dted_buffer(tileCoords, offset, length)
    Core->>Core: Parseia binário DTED e insere matriz no Grid Index
    Core-->>SDK: Sucesso (Tile registrado no Cache Espacial)

    Note over Host, Core: Fase 2: Alertas em Tempo Real (MSAW)
    Host->>SDK: checkAltimetry(aircraftId)
    SDK->>Core: get_terrain_elevation(lat, lon)
    Core->>Core: Acesso O(1) no Grid Index do cache ativo
    Core-->>SDK: altitude_solo (metros WGS84)
    SDK->>SDK: Compara: (aeronave_alt - altitude_solo) < Margem de Segurança?
    SDK-->>Host: Retorna Alerta MSAW (Verdadeiro/Falso)

    Note over Host, Core: Fase 3: Geração de Perfil Vertical (Visão 2.5D)
    Host->>SDK: getFlightVerticalProfile(routePoints, samplingStep)
    SDK->>Core: compute_vertical_profile(routePoints, samplingStep)
    loop Para cada ponto amostrado na rota
        Core->>Core: Consulta altitude do solo no indexador DTED
    end
    Core-->>SDK: Array de coordenadas de perfil [distancia_acumulada, altitude_solo]
    SDK-->>Host: Retorna dados para plotagem do perfil de voo 2.5D
```

---

## 5. Decisões Arquiteturais Críticas (ADRs)

### ADR-001: Pipeline Híbrido de Renderização (Matrizes vs Vértices)
* **Contexto:** Desenhar mapas complexos com vetores geográficos gera milhões de vértices. Por outro lado, alvos de radar (aviões) exigem símbolos rotacionados de forma fixa e etiquetas legíveis sem distorção 3D (efeito *Billboard*).
* **Decisão:** Adotou-se o modelo híbrido.
  * O fundo do mapa (MVT) e o terreno denso são projetados e renderizados na GPU usando transformações de matrizes $4\times4$ computadas no Rust Core.
  * Os símbolos de aviões e as etiquetas textuais dinâmicas são projetados de geodésico para coordenadas de tela de 2D $(X,Y)$ no Rust Core. O desenho em si ocorre de forma "achatada" e pixel-perfect na tela, permitindo algoritmos de prevenção de sobreposição de texto (anti-cluttering) eficientes na CPU.
* **Consequência:** Excelente desempenho gráfico global combinado com legibilidade absoluta e segurança no controle de telas ATC.

### ADR-002: Ingestão Passiva de Recursos no Core Rust (WASM)
* **Contexto:** Arquivos DTED de terreno e estilos SLD residem em disco ou em servidores geográficos externos. O código em WebAssembly executando nos browsers padrão possui restrições severas de segurança para I/O nativo (file system) e requisições HTTP diretas por parte do Core Rust podem inflar o binário final desnecessariamente.
* **Decisão:** O Core em Rust é completamente passivo. Ele não possui drivers de rede ou leitores de disco. A SDK TypeScript baixa os recursos (buffers MVT, XML de arquivos SLD e ArrayBuffers DTED) via APIs nativas do navegador (`fetch`) e injeta os ponteiros de memória binária nos métodos expostos pelo WebAssembly.
* **Consequência:** Binário WASM leve, desacoplamento total da lógica de transporte de dados e segurança de execução aprimorada.

### ADR-003: Interpolação de Movimento no Lado do Cliente (Dead Reckoning)
* **Contexto:** Feeds de radar ou ADS-B chegam à aplicação host com intervalos de 1 a 4 segundos. Atualizar as aeronaves na tela diretamente nesses pings causará animações travadas e desconforto visual aos controladores.
* **Decisão:** Implementar a lógica de estimativa cinemática no Core. O Host apenas reporta as posições reais com seus timestamps históricos. O Core realiza o cálculo de predição linear da posição atual da aeronave com base no tempo de processamento do frame e na velocidade/rumo informados.
* **Consequência:** Movimento contínuo e suave a 60 FPS, mesmo sob redes instáveis ou atrasos na recepção de pacotes.

### ADR-004: Gerenciamento de Ciclo de Vida e Desalocação de Memória WebAssembly
* **Contexto:** O WebAssembly (WASM) compartilha uma memória linear com o JavaScript. Objetos criados em Rust (como structs instanciadas via wrapper do `wasm-bindgen`) residem no heap do WASM e não são gerenciados pelo Garbage Collector (GC) do JavaScript. Se a SDK TypeScript instanciar objetos no Rust e perder as referências no JS sem liberá-los explicitamente, a memória do WASM crescerá indefinidamente, gerando *out-of-memory* em execuções de longa duração (essenciais em sistemas ATC).
* **Decisão:** A SDK TypeScript implementará um controle rígido do ciclo de vida dos objetos Rust/WASM.
  - Toda estrutura criada no Rust que possua ciclo de vida curto (ex: alvos descartados, perfis de voo de consulta rápida) deverá ter seu método `.free()` invocado explicitamente pela SDK TS.
  - Para buffers densos e de tamanho variável (como grids DTED de terreno carregados), a SDK gerenciará um cache de tamanho fixo com política de substituição LRU (Least Recently Used). Quando um tile de terreno for descartado do cache, a SDK notificará o Core Rust para liberar a memória correspondente.
  - O Core Rust usará vetores estáticos pré-alocados para dados altamente dinâmicos (como a lista de alvos interpolados no frame atual), evitando alocações e desalocações repetidas de memória a cada frame de renderização.
* **Consequência:** Estabilidade de uso de memória a longo prazo, previsibilidade de consumo de RAM do navegador e prevenção de travamentos por exaustão de memória em sessões operacionais contínuas.

### ADR-005: Segregação de Camadas de Exibição e Otimização Gráfica (Texture Atlases & Framebuffer Cache)
* **Contexto:** Desenhar mapas completos contendo milhões de polígonos GIS estáticos e texturas de relevo juntamente com alvos dinâmicos em tempo real a 60 FPS causa alta sobrecarga na GPU e CPU devido a trocas frequentes de contexto e excesso de draw calls. Símbolos militares complexos (NATO APP-6) compostos por múltiplos sub-vetores agravam esse problema se renderizados individualmente a cada frame.
* **Decisão:** O framework adotará uma estratégia de renderização segregada por camadas:
  - **Separação de Ciclos:** As camadas estáticas de fundo de mapa (MVT e elevação) serão renderizadas e compostas em Framebuffers fora da tela (Offscreen Render Targets) apenas quando a câmera sofrer alteração física. Se a tela estiver estática, a GPU realiza apenas o redesenho rápido dessa textura cacheada (*blitting*).
  - **Atlas de Texturas Dinâmico:** Os símbolos complexos decodificados pelo `Symbol Registry` serão rasterizados uma única vez na CPU e injetados em um Atlas de Texturas comum na GPU.
  - **Instanciamento:** Para desenhar milhares de aeronaves e alvos, a SDK enviará um único buffer de dados dinâmicos e fará uma chamada de desenho instanciada (`drawElementsInstanced`) baseada nos offsets de textura do Atlas, reduzindo milhares de draw calls para apenas uma.
* **Consequência:** Alta taxa de quadros (60 FPS estáveis), tempo de CPU livre na thread principal para processamento tático e baixíssimo consumo de bateria/recursos em painéis de monitoramento estáticos.

### ADR-006: Importação e Resolução de Símbolos Customizados (SVG e PNG)
* **Contexto:** Além das simbologias profissionais procedurais padrão (ICAO/NATO), a aplicação host precisa injetar e renderizar ícones customizados fornecidos nos formatos vetorial (SVG) ou rasterizado (PNG). O framework necessita de um fluxo que unifique estas fontes externas e mantenha a consistência de renderização e performance em visualizações 2D e 3D.
* **Decisão:** O componente `Symbol Registry` e as SDKs processarão as importações de forma a acoplá-las ao ecossistema do Texture Atlas:
  - **Ingestão de PNG:** O arquivo de imagem rasterizada é decodificado em buffer de pixels nativo (na CPU) e enviado para inserção direta em uma sub-região livre do *Texture Atlas* na GPU.
  - **Ingestão de SVG:** Para evitar o alto custo de desenhar caminhos vetoriais na GPU em tempo de execução, o SVG é rasterizado pela SDK na CPU antes do envio para a GPU. No ambiente Web, isso é realizado desenhando o SVG em um canvas offscreen na escala requerida; no ambiente nativo desktop, utiliza-se uma biblioteca leve de rasterização CPU (como `resvg`/`tiny-skia` em Rust).
  - **Unificação nos Streams 2D/3D:** Uma vez carregados no Texture Atlas com suas respectivas coordenadas UV, os símbolos importados utilizam o mesmo pipeline de renderização instanciada. No fluxo 2D, são desenhados como sprites planos comuns. No fluxo 3D, são renderizados usando *Billboard Shaders* que alinham as coordenadas planas à câmera, impedindo distorções tridimensionais de perspectiva e garantindo legibilidade.
* **Consequência:** Flexibilidade na customização visual, independência de formatos externos durante o ciclo de renderização ativa, e preservação da escalabilidade vetorial (no caso do SVG) ao rasterizar sob demanda na resolução ideal do dispositivo (suporte nativo a telas High-DPI/Retina).

---

## 6. Mapeamento da Estrutura de Diretórios com Componentes

A estrutura física proposta para o repositório é organizada conforme a divisão de responsabilidades da arquitetura:

```text
olayer/
├── core/                         # [C4 Component: Olayer Core Engine]
│   ├── Cargo.toml
│   └── src/
│       ├── geodesy/              # Módulo de Fórmulas Geodésicas e ECEF (WGS84)
│       │   └── mod.rs
│       ├── projections/          # Implementações de Estereográfica, LCC e Mercator
│       │   └── mod.rs
│       ├── terrain/              # Parse de arquivos DTED e Índice de Altitude O(1)
│       │   └── mod.rs
│       ├── sld/                  # Parser de XML para Estilização SLD
│       │   └── mod.rs
│       └── interpolator/         # Lógica de Dead Reckoning para rastreio de alvos
│           └── mod.rs
│
├── bindings/
│   └── wasm/                     # [C4 Component: WASM Bindings Layer]
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs            # Exportações com #[wasm_bindgen] para a SDK TS
│
└── sdk/
    ├── ts/                       # [C4 Component: Olayer TS SDK]
    │   ├── package.json
    │   ├── src/
    │   │   ├── controller/       # Gerenciamento de Loop, FPS Throttler e Eventos
    │   │   ├── providers/        # Chamadas de rede WMTS, MVT, SLD e injeção DTED
    │   │   ├── renderer/         # Renderizador WebGL (GPU) e Canvas (CPU)
    │   │   └── index.ts          # API pública da SDK TypeScript
    │   └── tsconfig.json
    │
    └── rust/                     # [C4 Component: Olayer Native SDK]
        ├── Cargo.toml
        └── src/
            └── lib.rs            # Interface estática / wgpu nativo para Desktop
```

---

## 7. Próximos Passos de Validação de Arquitetura

Para ratificar as premissas deste documento de arquitetura, as seguintes atividades experimentais são planejadas:
1. **Validação Matemática (Geodesia):** Criação de testes unitários no módulo `geodesy` comparando a distância geodésica entre aeroportos conhecidos calculada pelo core com o modelo de referência oficial do WGS84.
2. **Benchmark WASM-TS Bound:** Medição da latência de transferência de dados ao carregar buffers DTED de 1MB entre a pilha TypeScript e a memória linear do WASM para confirmar a ausência de gargalos na borda.
3. **Teste de Projeção Dinâmica:** Renderização de um setor de testes com troca rápida em tempo de execução de Lambert Conformal Conic para Estereográfica Azimutal para garantir a atualização correta das matrizes e vértices.
