
# Especificação Técnica: Olayer
# Framework GIS Híbrido para Controle de Tráfego Aéreo (ATC)

## 1. Visão Geral e Escopo

O objetivo deste projeto é o desenvolvimento de um framework GIS de missão crítica para cenários de controle de tráfego aéreo (ATC) em 2D, com suporte nativo a visões em 2.5D (Perfil de Voo) e 3D (Globo Digital).

O framework deve ser **estritamente focado no domínio GIS** (processamento geográfico, transformações matemáticas, renderização e indexação de terreno), delegando inteiramente a ingestão de dados brutos de rede (como protocolos Asterix ou feeds ADS-B) para a aplicação hospedeira (*Host*).

---

## 2. Premissas Arquiteturais e Stack Tecnológica

Para garantir segurança de memória, portabilidade e performance próxima à nativa em ambiente web e local, o projeto adotará uma abordagem multi-linguagem:

* **Core Agnóstico (Rust):** Todo o motor de cálculo geodésico, algoritmos de projeção, parser de estilos SLD e indexação de arquivos DTED serão escritos em Rust puro.
* **Distribuição Híbrida (WebAssembly + Native):** * **Navegadores:** O Core em Rust será compilado para **WebAssembly (WASM)**, provendo uma camada de bindings para ser consumida via **TypeScript**.
* **Sistemas Locais:** O Core será consumido diretamente como uma dependência nativa em Rust.


* **Abstração Matemática:** O motor de cálculo será 100% agnóstico de projeção de tela. Ele operará exclusivamente em coordenadas geodésicas baseadas no elipsoide WGS84 ($\phi, \lambda, h$) e coordenadas cartesianas geocêntricas ECEF ($X, Y, Z$) com precisão de ponto flutuante de 64 bits (`f64`).

---

## 3. Design de Camadas (Arquitetura)

```
+---------------------------------------------------------------+
|                      Camada de Aplicação                      |
|       (Host App: TypeScript Web / Rust Local Aplicativo)      |
+---------------------------------------------------------------+
                               |
                               v
+---------------------------------------------------------------+
|                 Camada de Abstração Visual                    |
|       (Pipeline Híbrido: Matrizes GPU & Coordenadas CPU)      |
+---------------------------------------------------------------+
                               |
                               v
+---------------------------------------------------------------+
|                Core Agnóstico em Rust (WASM)                  |
|     (Cálculos Geodésicos, Predição de Estado, Cache DTED)     |
+---------------------------------------------------------------+
                               |
                               v
+---------------------------------------------------------------+
|                      Provedores de Dados                      |
|         (Buffers MVT/WMS do GeoServer, Buffers DTED)          |
+---------------------------------------------------------------+

```

---

## 4. Pipeline de Renderização Híbrido

Para otimizar o balanço entre performance gráfica de larga escala e precisão na plotagem de alvos, o framework implementará uma **estratégia híbrida de renderização**:

### A. Canal de Matrizes (Orientado à GPU)

* **Uso:** Renderização de terreno denso (DTED) e mapas de fundo vetoriais ou ráster originados do **GeoServer** (MVT - Mapbox Vector Tiles / WMS).
* **Mecanismo:** O Core calcula e exporta matrizes de transformação $4 \times 4$ baseadas na projeção ativa e estado da câmera. A aplicação *Host* injeta estas matrizes diretamente nos Shaders da GPU (WebGL / WebGPU / Vulkan). Operações de *zoom* e *pan* atualizam a matriz sem reprocessar os vértices na CPU.

### B. Canal de Vértices Projetados (Orientado à CPU)

* **Uso:** Renderização de plotas de radar, etiquetas de dados (*data blocks*), vetores de rumo e símbolos de alvos dinâmicos.
* **Mecanismo:** A interpolação física dos alvos (Dead Reckoning) ocorre estritamente em coordenadas geodésicas 3D no elipsoide WGS84. Para a renderização, a SDK do cliente (TypeScript ou Native) consulta as posições geodésicas interpoladas (`LatLon`) e as converte em coordenadas de tela $(X, Y)$ e profundidade utilizando o resolvedor de projeção ativo (Projections Engine) do Olayer Core.
* **Vantagem Operacional:** Mantém a lógica cinemática completamente agnóstica de exibição, evita distorções de perspectiva nos símbolos em visualizações 3D (efeito *Billboard* automático na renderização) e permite a execução de algoritmos de anti-sobreposição (*anti-cluttering*) de etiquetas na CPU de forma estável.

### C. Estrutura de Renderização Baseada em Camadas (Layer Stack)

Para prover flexibilidade operacional e otimizar a carga de trabalho de rendering, a visualização é estruturada em uma pilha de **Camadas (Layers)** com ciclos e frequências de repintura segregados:
* **Camadas Dinâmicas (Alvos Táticos, Radar Meteorológico e Réguas Interativas):** Atualizadas em tempo real em cada ciclo de animação da tela (até 60 FPS) sobrepondo-se à textura composta das camadas estáticas, sem custo de reprocessamento do fundo.

---

## 5. Requisitos de Funcionalidades GIS

### 5.1 Suporte a Projeções, Visões e Controle de Câmera

O framework deve suportar a alternância dinâmica em tempo de execução entre as seguintes projeções cartográficas e modos de exibição, com gerenciamento unificado através do **Camera Engine**:

* **Estereográfica Azimutal:** Foco em radares de aproximação (TMA) e preservação de ângulos locais.
* **Lambert Conformal Conic (LCC):** Foco em mapas de rota En-Route de longa distância.
* **Mercator / Web Mercator:** Compatibilidade macro padrão.
* **Visão 2.5D (Mapa de Perspectiva Plana Inclinada):** Projeção perspectiva tridimensional sobreposta a um plano projetado. Utiliza uma inclinação (pitch/tilt) padrão de **35 graus** (perspectiva declinada de topo/bird's-eye view, melhorando a visualização de alvos e relevo em comparação com o antigo ângulo estático de 55 graus).
* **Visão 3D (Globo Virtual):** Transformação direta de coordenadas elipsoidais para cartesianas ECEF.

#### Controle Dinâmico da Câmera (Zoom, Bearing, Pitch, Roll)
O **Camera Engine** (`core::camera`) do Olayer Core provê controle unificado sobre a atitude da câmera em radianos, integrado às seguintes matrizes View-Projection:
- **Zoom (escala linear):** Aplicado nos modos 2D, 2.5D e 3D.
- **Bearing / Rotação (yaw):** Controla a orientação azimutal da câmera nos modos 2D, 2.5D e 3D.
- **Pitch / Tilt (inclinação vertical):** Controla a inclinação do horizonte nos modos 2.5D (0° a 85°) e 3D (-90° a 90°).
- **Roll (rolagem lateral):** Disponível nos modos 2.5D e 3D para suporte a movimentação em atitude de vôo completa.

### 5.2 Simbologia Padronizada (ICAO e NATO) e Registro de Símbolos

O framework gerencia bibliotecas de símbolos profissionais para aviação civil e defesa de forma performática e modular, dividindo as responsabilidades entre compilação offline de vetores (SVG) e carregamento dinâmico de imagens rasterizadas (PNG):

* **Compilação e Importação de Símbolos Vetoriais (SVG):**
  - Para manter a leveza do Core WASM e evitar o uso de interpretadores pesados de SVG em tempo de execução, a importação e tratamento de arquivos SVG são feitos no processo de build por meio da ferramenta CLI **`tools/symbol-compiler`**.
  - O compilador faz o parse recursivo de caminhos, círculos, textos e estilos (incluindo cores CSS, opacidades e tracejados) dos arquivos SVG, mapeando-os para um JSON de biblioteca declarativa no padrão `DeclarativeLibraryDto` do Core.
  - O Core Rust consome esta biblioteca no formato consolidado através do `DeclarativeProvider` registrado no `SymbolRegistry`.
* **Carregamento de Símbolos Rasterizados (PNG/JPG) via SDK:**
  - Símbolos contendo imagens PNG ou JPG são injetados diretamente na SDK TypeScript através do método `TextureAtlasManager::registerImageSymbol`. 
  - A SDK utiliza as APIs nativas do navegador para carregar e renderizar os pixels de forma assíncrona, desenhando-os diretamente no Canvas do Texture Atlas para upload na GPU. A lógica de decodificação raster fica inteiramente a cargo do browser, não alterando a estrutura do Core em WASM.
* **Simbologia Civil (ICAO) e Militar (NATO APP-6 / MIL-STD-2525):**
  - Os pacotes de símbolos civis padrão (VOR, NDB, DME, TACAN, etc.) e táticos militares (molduras de afiliação, ícones de caça, cargueiro, etc.) são providos como SVGs base modulares pré-compilados pela ferramenta de build ou injetáveis via JSON compilado.
* **Estratégia de Performance (Atlas de Texturas & Instanciamento):**
  - Para evitar a sobrecarga de draw calls, a SDK compila os símbolos gerados sob demanda (primitivos vetoriais do WASM ou imagens carregadas via PNG) em uma textura compartilhada única na GPU (**Texture Atlas / Spritesheet**).
  - A plotagem de milhares de alvos de radar faz uso de uma única chamada de desenho instanciada (`drawElementsInstanced`) que referencia as coordenadas UV do Atlas, eliminando gargalos de CPU e transferências extras.
  - O renderizador final aplica *Billboard Shaders* para manter os símbolos planos e orientados de frente para o controlador, mesmo em visualizações 3D ou 2.5D inclinadas do globo.
* **Compatibilidade com Streams 2D/3D:**
  - O Texture Atlas e a projeção de símbolos são nativamente compatíveis com todos os modos de visão (2D plano, 2.5D de perfil e 3D do globo virtual). 
  - No fluxo **2D/2.5D**, os símbolos do Atlas são renderizados diretamente como sprites planos nas coordenadas de tela.
  - No fluxo **3D**, o renderizador projeta a origem tridimensional da aeronave e desenha os símbolos utilizando *Billboards* no espaço tridimensional, garantindo que permaneçam legíveis e com escala visual consistente sem distorção angular de perspectiva.
* **Estilização SLD:** O Core contém um parser XML para arquivos **SLD (Styled Layer Descriptor)** que converte regras estáticas de estilização em metadados de estilo aplicados dinamicamente sobre as regras dos símbolos resolvidos no Registro de Símbolos.

### 5.3 Integração Passiva com DTED

* O motor GIS não fará requisições de I/O para ler arquivos do disco no modo web. Ele aceitará a injeção passiva de pedaços de elevação via buffers de memória (`ArrayBuffer` ou estruturas mapeadas).
* O Core proverá buscas de complexidade $O(1)$ para determinar a altitude do solo e calcular o *Clearance* vertical de segurança de uma aeronave (alertas de MSAW).

---

## 6. Sincronismo de Tempo e Controle Dinâmico de FPS

O sistema deve desacoplar rigidamente o recebimento de dados do sensor (tipicamente na taxa de 1 Hz) da renderização na tela, permitindo o controle estrito de frames por segundo (FPS).

### 6.1 Interpolação Preditiva (Dead Reckoning)

Os alvos dinâmicos serão registrados no Core Rust através de um **Vetor de Estado** (`TargetState`), contendo a posição elipsoidal do último *ping* (WGS84 `LatLon`), rumo real em radianos, velocidade horizontal em metros/segundo, velocidade vertical em metros/segundo e o *timestamp* da captura.
Quando a aplicação *Host* solicitar a renderização de um frame, ela passará o *timestamp* atual do sistema. O Core computará a posição estimada do alvo (3D geodésico) de forma linear e suave (utilizando o elipsoide de referência e as funções do `Geodesy Engine`) entre as atualizações de sensor, sem acoplamento com projeções.

### 6.2 Gerenciamento de FPS

A aplicação *Host* controlará o passo de tempo (*time-step throttling*), permitindo alterar a taxa de atualização dinamicamente para preservação de recursos do hardware:

* **Modo Econômico (Ex: 15-20 FPS):** Ativado quando a tela e a câmera estão estáticas. A suavidade do movimento das aeronaves é mantida via interpolação.
* **Modo Responsivo (Ex: 60 FPS):** Ativado sob demanda via eventos de interface do usuário (enquanto o controlador arrasta o mapa ou altera o zoom), retornando ao modo econômico de forma automática após a estabilização da tela.
---

## 7. Infraestrutura de Servidor de Mapas (Data Providers)

Para alimentar o framework com dados cartográficos e estruturais de aviação, o projeto adotará a seguinte pilha de servidores:

### 7.1 Servidor de Aplicação: GeoServer
* **Versão Recomendada:** 2.22.x ou superior (com suporte estável a extensões de Vector Tiles).
* **Protocolos Consumidos:** * **WMTS / MVT (Mapbox Vector Tiles):** Usado para a carga massiva de fundos de mapa vetoriais (fronteiras, litorais, áreas urbanas) e aerovias de alta densidade. O Core Rust aplicará a projeção ativa (ex: LCC) sobre os vértices do MVT.
  * **WFS (Web Feature Service):** Usado para consultas pontuais de metadados críticos (ex: buscar coordenadas exatas de uma cabeceira de pista ou informações de um rádio-auxílio/VOR).
* **Estilização:** O GeoServer centralizará os arquivos `.sld` estruturais que o framework consumirá via API para sincronizar a identidade visual.

### 7.2 Armazenamento: PostgreSQL + PostGIS
* O banco de dados geográfico armazenará feições espaciais complexas com indexação `GIST` para otimizar requisições de renderização de setores.

### 7.3 Otimização de Entrega: GeoWebCache (GWC)
* Toda requisição de mapa de fundo vinda do framework deve obrigatoriamente bater na camada do GeoWebCache em formato MVT ou WMTS (Ráster, para imagens de satélite). Fica proibido o uso de WMS puro/dinâmico para telas operacionais em tempo real para evitar sobrecarga do servidor de mapas.

### 7.4 Pilha de Dados de Mapa (Map Data Stack)
Para isolar a rede e o gerenciamento de arquivos da renderização WebGL e cálculos do radar, as SDKs implementam a pilha de dados baseada em `MapDataSource`:
* **`VectorTileSource` (MVT / GeoServer):** Gerencia a paginação e o cálculo geométrico dos limites visíveis (Bounding Box) da câmera em tempo real, realizando buscas paralelas de blocos vetoriais no GeoServer.
* **`RasterTileSource` (WMTS / OSM):** Controla o download de imagens de mapa e o upload de texturas para a GPU de forma assíncrona.
* **`TerrainTileSource` (DTED / Terreno):** Paginação automática baseada na posição do controlador. Substitui a injeção passiva pura por um resolvedor de rede dinâmico com fila de downloads e algoritmo de despejo de memória LRU (Least Recently Used) para garantir consumo estável de memória RAM/WASM.
* **Desacoplamento por Concorrência:** A decodificação de formatos geográficos complexos (MVT/DTED) será executada em threads de suporte (Web Workers no navegador, Threads locais em desktop) para que a thread de renderização principal nunca bloqueie o tráfego do radar.

---

## 8. Estrutura Proposta de Código do Repositório

```text
├── core/                  # Código Rust Puro (Agnóstico e Matemático)
│   ├── Cargo.toml
│   └── src/
│       ├── geodesy/       # Módulo de Fórmulas Geodésicas e ECEF (WGS84)
│       ├── camera/        # Gerenciamento de CameraState e matrizes View-Proj para 2D/2.5D/3D
│       ├── terrain/       # Parse de arquivos DTED e Índice de Altitude O(1)
│       ├── sld/           # Parser XML de arquivos Styled Layer Descriptor
│       └── projections/   # Algoritmos das projeções (Estereográfica, LCC, Mercator)
│
└── sdk/
    ├── ts/                # SDK TypeScript para Navegadores
    │   └── wasm/          # Camada de exportação wasm-bindgen para TypeScript
    │
    └── native/            # SDK Nativo Desktop e C-FFI
        ├── c_ffi_bridge/  # Exportação C-FFI (cbindgen)
        └── desktop/       # Aplicação nativa desktop (WGPU/winit/egui)
```

---




