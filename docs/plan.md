# Plano de Desenvolvimento: Olayer
## Cronograma de Implementação e Milestones do Framework GIS Híbrido

Este documento estabelece o plano de desenvolvimento modular para a construção do **Olayer**, dividindo o projeto em fases incrementais e marcos de entrega (*milestones*). O planejamento segue a separação estrita de responsabilidades definida na [Arquitetura do Sistema (arch.md)](file:///c:/Users/rafae/projects/rust/olayer/docs/arch.md) e na [Especificação Técnica (spec.md)](file:///c:/Users/rafae/projects/rust/olayer/docs/spec.md).

---

## 🗺️ Visão Geral do Roteiro (Roadmap)

```mermaid
gantt
    title Cronograma Estimado do Olayer
    dateFormat  YYYY-MM-DD
    section Fase 1: Core Geodésico
    Matemática e Geodésia (WGS84)   :active, des1, 2026-06-01, 10d
    section Fase 2: Projeções & Estilos
    Projeções LCC/Estereográfica     : des2, after des1, 12d
    Parser de Estilos SLD            : des3, after des1, 8d
    section Fase 3: Terreno & Alvos
    Engine DTED O(1) e Perfil 2.5D   : des4, after des2, 15d
    Dead Reckoning (Interpolator)    : des5, after des2, 8d
    section Fase 4: Bindings WASM
    Ponte wasm-bindgen & Memória     : des6, after des5, 10d
    section Fase 5: SDK TypeScript
    Loop de Controle (FPS) & Canvas  : des7, after des6, 12d
    Pipelines de Renderização (GPU/CPU): des8, after des7, 18d
    section Fase 6: SDK Desktop Nativo
    Wrapper wgpu / Nativo            : des9, after des8, 15d
    section Fase 7: Integração & QA
    Testes de Carga & Validação      : des10, after des9, 10d
```

---

## 🛠️ Detalhamento das Fases

### Fase 1: Core Geodésico e Matemática WGS84
* **Objetivo:** Estabelecer a precisão matemática do framework, implementando o cálculo geodésico puro.
* **Tarefas:**
  * [ ] Criar estrutura de dados para coordenadas Geodésicas $(\phi, \lambda, h)$ e ECEF $(X, Y, Z)$ em precisão `f64`.
  * [ ] Implementar conversões bidirecionais geodésicas $\leftrightarrow$ ECEF.
  * [ ] Implementar cálculos de distância de grande círculo (Fórmula de Vincenty / Haversine) e Azimute.
  * [ ] Criar suite de testes unitários validando contra dados reais do elipsoide WGS84.
* **Plano de Testes:**
  * [ ] Executar suite de testes unitários em Rust comparando os cálculos de distância e conversões com ferramentas de referência (GeographicLib / PROJ) exigindo tolerância menor que 1mm.
  * [ ] Validar comportamento matemático em pontos limítrofes (polos geográficos, Equador e transição do Antimeridiano +-180°).
* **Marcos (Milestone 1):** Validação matemática aprovada, com erro acumulado menor que 1 milímetro.

### Fase 2: Projeções Cartográficas, Parser SLD e Biblioteca de Símbolos
### Fase 2: Projeções Cartográficas, Parser SLD, Biblioteca de Símbolos e Camera Engine
* **Objetivo:** Permitir a tradução de coordenadas do globo para planos bidimensionais, configurar a engine de estilização, resolver simbologias estruturadas e gerenciar dinamicamente a atitude da câmera.
* **Tarefas:**
  * [ ] Implementar a projeção **Lambert Conformal Conic (LCC)** com paralelos padrão configuráveis.
  * [ ] Implementar a projeção **Estereográfica Azimutal** com foco no centro do radar (TMA).
  * [ ] Implementar a projeção **Web Mercator** (EPSG:3857) para fundos de mapa padrão.
  * [ ] Desenvolver o parser XML para o padrão OGC **SLD (Styled Layer Descriptor)**, traduzindo estilos geográficos em dicionários de estilos simples.
  * [ ] Desenvolver o módulo **`Symbol Registry`** para decodificação e validação de códigos táticos militares (SIDC NATO) e auxílios-rádio civis (ICAO).
  * [ ] Desenvolver o componente **`Camera Engine`** (`core::camera`) gerenciando `CameraState` (com zoom, bearing, pitch, roll) e calculando as matrizes de View-Projection para os modos 2D, 2.5D (com inclinação declinada) e 3D.
* **Plano de Testes:**
  * [ ] Executar testes de reprojeção cruzada (projetar e desprojetar pontos conhecidos para verificar reversibilidade matemática).
  * [ ] Testar parse de SLD contendo tags inválidas ou corrompidas para certificar que o parser não causa pânicos na aplicação.
  * [ ] Validar a correta resolução de SIDC NATO (APP-6) para afiliações e tipos variados, testando comportamento diante de códigos SIDC desconhecidos ou mal-formados.
  * [ ] Validar matrizes geradas pelo `Camera Engine` verificando o correto mapeamento de pontos conhecidos para o espaço NDC sob rotação, inclinação e escala.
* **Marcos (Milestone 2):** Algoritmos de projeção validados, parser SLD lendo arquivos sem exceções, gerador de símbolos composto estruturado e Camera Engine provendo matrizes matemáticas robustas.

### Fase 3: Engine de Terreno DTED e Interpolação de Alvos
* **Objetivo:** Adicionar elevação geográfica passiva e estimativa cinemática contínua de movimento das aeronaves.
* **Tarefas:**
  * [ ] Criar indexador espacial em memória (Grid Index) para arquivos binários DTED.
  * [ ] Implementar busca por coordenada geográfica em tempo $O(1)$ retornando a altitude do solo.
  * [ ] Desenvolver algoritmo de perfil de corte vertical do terreno (Visão 2.5D).
  * [ ] Implementar o módulo `Target Interpolator` (Dead Reckoning) usando cinemática linear para suavizar trajetórias de alvos dinâmicos.
* **Plano de Testes:**
  * [ ] Testar a consistência do parser de arquivos DTED a partir de buffers binários falsificados na memória.
  * [ ] Validar a precisão física da interpolação cinemática em intervalos de tempo fracionados (ex: verificar posição estimada no instante $T+0.250s$).
  * [ ] Medir a latência do cálculo de MSAW para garantir integridade sob alta taxa de consultas operacionais.
* **Marcos (Milestone 3):** Simulação de radar interpolada e checagem de altimetria operando em tempo constante no Core Rust.

### Fase 4: Camadas de Interoperabilidade (WASM e C-FFI) e Gestão de Memória
* **Objetivo:** Preparar o Core Rust para ser consumido tanto em navegadores (via WASM/TS) quanto em hosts locais (C++/C via FFI), garantindo vazamento zero de recursos.
* **Tarefas:**
  * [ ] Configurar a compilação do `wasm-pack` e expor funções e estruturas do Core via `wasm-bindgen`.
  * [ ] Integrar o `cbindgen` no processo de build nativo para gerar cabeçalhos C-compatíveis (`libolayer_native.h`) a partir das diretivas FFI do Core.
  * [ ] Implementar pontes otimizadas para transferência de grandes volumes de dados (buffers binários DTED e MVT) usando memória linear compartilhada.
  * [ ] Implementar política de desalocação explícita de memória (chamadas a `.free()`) na SDK TS e mapeamento de destruidores nativos FFI (conforme ADR-004).
  * [ ] Configurar cache LRU de tiles DTED com expurgo de memória ativa na heap do WASM e cache nativo.
* **Plano de Testes:**
  * [ ] Executar testes automatizados no navegador headless via `wasm-bindgen-test` para homologar as assinaturas WebAssembly.
  * [ ] Compilar um programa em C++ simples para validar os cabeçalhos `.h` autogerados e atestar a correta passagem de structs geodésicas via FFI.
  * [ ] Testes de vazamento (Leak Checking) automatizados monitorando o crescimento do heap do WASM/C após criar e destruir massivamente estruturas dinâmicas.
* **Marcos (Milestone 4):** Pacotes WASM e bibliotecas dinâmicas/estáticas nativas geradas com FFI validado e testes de vazamento aprovados.

### Fase 5: SDK TypeScript (Ambiente Web)
* **Objetivo:** Construir o framework visual consumido na Web, gerenciando o ciclo de exibição e interação.
* **Tarefas:**
  * [ ] Configurar repositório TS (Vite, esbuild, TypeScript) e carregar o módulo WASM de forma assíncrona.
  * [ ] Desenvolver o `TS Controller` e o loop de renderização inteligente com suporte a taxas de FPS dinâmicas (15 FPS ocioso / 60 FPS ativo).
  * [ ] Criar o **`Layer Manager`** para coordenar a pilha de camadas (Layer Stack) e segregar a repintura das camadas estáticas e dinâmicas.
  * [ ] Criar o `Data Provider Manager` para requisições assíncronas do GeoServer (MVT, WMTS, SLD) e servidor de terreno (DTED).
  * [ ] Desenvolver o `GPU Pipeline` (WebGL 2.0 / WebGPU) para renderização em tempo real de matrizes, terreno e camadas vetoriais MVT com cache em texturas de Framebuffer.
  * [ ] Implementar a compilação dinâmica do **Texture Atlas** na GPU e renderização de alvos via chamadas instanciadas (`drawElementsInstanced`).
  * [ ] Desenvolver o `CPU Pipeline` para plotagem pixel-perfect de alvos e implementar o algoritmo de **Anti-cluttering** de etiquetas na thread do navegador.
  * [ ] Implementar controles de câmera interativos na interface (zoom, bearing, pitch, roll) e sincronização bidirecional com gestos de mouse (botão direito / Shift+drag para inclinar e rotacionar).
* **Plano de Testes:**
  * [ ] Testes de regressão visual automatizados (Snapshot Testing) comparando capturas de tela do Canvas 2D/WebGL contra frames de referência aprovados.
  * [ ] Testar unitariamente a segregação de renderização das camadas estáticas vs dinâmicas no `Layer Manager`.
  * [ ] Simular eventos contínuos de mouse/pan para verificar se o `TS Controller` de fato limita a taxa de quadros e retorna ao modo ocioso (15 FPS) de forma autônoma.
* **Marcos (Milestone 5):** Tela radar funcional no navegador rodando a 60 FPS com renderização híbrida ativa e controle dinâmico total da câmera.

### Fase 6: SDK Rust Nativo (Ambiente Desktop)
* **Objetivo:** Viabilizar o uso do framework em aplicativos desktop locais de altíssima performance (Rust nativo e FFI).
* **Tarefas:**
  * [ ] Implementar o wrapper local da SDK conectando diretamente com as APIs estáticas do Core Rust (sem WASM).
  * [ ] Desenvolver o `Native Controller` utilizando a crate `winit` para gerenciamento nativo de loop de renderização e janelas.
  * [ ] Criar o `Native Layer Manager` para controle e composição da pilha de camadas (estáticas e dinâmicas) no ambiente desktop.
  * [ ] Desenvolver a pipeline gráfica local utilizando a biblioteca `wgpu` para suporte a Vulkan, Metal e DirectX 12.
  * [ ] Implementar a compilação dinâmica do Texture Atlas local e renderização instanciada via pipeline nativo do `wgpu`.
  * [ ] Configurar a leitura assíncrona direta de arquivos DTED no disco rígido local da aplicação.
  * [ ] Adaptar a SDK nativa e controles locais para expor a atitude completa de câmera (zoom, bearing, pitch, roll).
* **Plano de Testes:**
  * [ ] Executar testes de renderização nativos salvando buffers wgpu locais como imagens PNG e fazendo diff visual.
  * [ ] Executar a suite de testes gráficos locais em ambientes CI utilizando adaptadores gráficos emulados por software (como o llvmpipe/lavapipe) para garantir funcionamento estável em headless.
* **Marcos (Milestone 6):** Aplicação Desktop nativa compilada com sucesso exibindo os mesmos recursos visuais da versão web, com suporte a FFI atestado.

### Fase 7: Testes Integrados, Validação Operacional e Benchmarks
* **Objetivo:** Homologar o framework em cenários reais simulados de alta carga.
* **Tarefas:**
  * [ ] Criar cenário de teste integrando o framework com um servidor **GeoServer** ativo.
  * [ ] Executar benchmark de stress injetando mais de 5.000 aeronaves ativas simultaneamente com Dead Reckoning a 60 FPS.
  * [ ] Medir e validar a latência de transferência de dados através da ponte JS/WASM.
  * [ ] Validar cenários de MSAW (alertas de colisão com o relevo) em tempo de execução.
* **Plano de Testes:**
  * [ ] Teste de Carga Extrema: Injeção contínua de 5.000+ alvos com atualização a 1 Hz, exigindo renderização a 60 FPS sem queda abrupta na taxa de atualização.
  * [ ] Teste de Endurance: Execução ininterrupta da aplicação por 24 horas simulando tráfego real para auditar crescimento de memória RAM no host e no navegador (ferramentas: Chrome DevTools Profiler / Valgrind / Instruments).
* **Marcos (Milestone 7):** Relatório de homologação técnica atestando estabilidade de FPS e consumo de memória estável após 24 horas de stress contínuo.

---

## 📈 Critérios de Aceitação de Entrega Geral

Para que o framework seja considerado pronto para produção:
1. **Zero Memory Leaks:** Sem vazamento de memória residual do WASM na heap ou na CPU em execução prolongada de 24 horas.
2. **Estabilidade de Frame Rate:** Manter 60 FPS estáveis durante interações de pan/zoom com mais de 2.000 aeronaves plotadas.
3. **Consistência Visual:** Alvos de radar, etiquetas e simbologias de SLD devem se comportar de maneira visualmente idêntica nos modos 2D, 2.5D e 3D.
