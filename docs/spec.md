
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

O framework deve prover suporte nativo a bibliotecas de símbolos profissionais para aviação civil e defesa, operando através de um **Registro de Símbolos (Symbol Registry)** no Core Rust e gerenciamento de renderização performático na SDK:
* **Simbologia Civil (ICAO):** Suporte completo à representação gráfica de auxílios-rádio à navegação (VOR, NDB, DME, TACAN), fixes de rota, aeródromos e pistas de pouso conforme as normas da ICAO.
* **Simbologia Militar (NATO APP-6 / MIL-STD-2525):** Suporte à decodificação e montagem procedural de símbolos táticos complexos a partir de códigos identificadores padrão (SIDC - Symbol Identification Codes), gerenciando molduras de afiliação, ícones centrais e modificadores.
* **Importação de Símbolos Customizados (SVG e PNG):**
  - O **Symbol Registry** deve aceitar a importação de símbolos customizados nos formatos **SVG** (vetorial) e **PNG** (rasterizado) para estender a biblioteca de ícones.
  - Os símbolos importados em SVG serão rasterizados em tempo de execução pela SDK na resolução apropriada (evitando perda de qualidade em telas Retina/High-DPI) antes do envio para o Texture Atlas. Os arquivos PNG serão decodificados e transferidos diretamente.
* **Estratégia de Performance (Atlas de Texturas & Instanciamento):**
  - Para evitar a sobrecarga de draw calls, a SDK compilará os símbolos gerados sob demanda (procedurais ou importados via SVG/PNG) em uma textura única compartilhada na GPU (**Texture Atlas / Spritesheet**).
  - A plotagem de milhares de aeronaves será feita em uma única chamada de desenho instanciada (`drawElementsInstanced`) referenciando as coordenadas UV do Atlas, eliminando gargalos de CPU.
  - O renderizador final aplicará *Billboard Shaders* para manter os símbolos planos e orientados de frente para o controlador, mesmo em visualizações 3D do globo.
* **Compatibilidade com Streams 2D/3D:**
  - A importação de SVG e PNG é nativamente compatível com os dois fluxos gráficos (2D plano e 3D globo virtual). 
  - No fluxo **2D**, os símbolos do Atlas são renderizados diretamente como sprites planos com coordenadas de tela $(X,Y)$.
  - No fluxo **3D**, o renderizador aplica projeção tridimensional nas posições de origem geodésicas das aeronaves, mas desenha os símbolos utilizando *Billboards* (placas planas orientadas de frente para a câmera), garantindo que imagens e vetores importados permaneçam legíveis e sem distorção perspectiva no globo 3D.
* **Estilização SLD:** O Core conterá um parser XML para arquivos **SLD (Styled Layer Descriptor)** que converterá regras estáticas em metadados de estilo para o Registro de Símbolos.

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
├── bindings/
│   └── wasm/              # Camada de exportação wasm-bindgen para TypeScript
│
└── sdk/
    ├── ts/                # SDK TypeScript para Navegadores (Wrappers Canvas/WebGL)
    └── rust/              # SDK Rust Nativo para aplicações Desktop (Wrappers wgpu/Vulkan)
```

---




