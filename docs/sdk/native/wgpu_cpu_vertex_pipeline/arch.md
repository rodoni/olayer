# Arquitetura: wgpu CPU/Vertex Pipeline

Este documento detalha o design arquitetural e a especificação técnica do componente **wgpu CPU/Vertex Pipeline** do SDK Nativo do Olayer.

---

## 1. Visão Geral

O **wgpu CPU/Vertex Pipeline** é responsável por calcular as coordenadas de tela bidimensionais $(X, Y)$ em pixels a partir de coordenadas geodésicas tridimensionais (latitude, longitude, altitude) de alvos de radar dinâmicos, desenhando-os sem distorções tridimensionais de perspectiva (efeito **Billboard**). Esse componente também gerencia os vetores de rumo, etiquetas de dados táticos (labels) e a renderização do perfil de voo 2.5D.

```mermaid
graph LR
    Target[Alvo Geodésico Lat/Lon/Alt] -->|Dead Reckoning| Interp[Posição Interpolada]
    Interp -->|Projeção na CPU| Screen[Coordenadas de Tela X/Y]
    Screen -->|Billboard Rendering| UI[Radar Overlay e Labels]
```

---

## 2. Algoritmo de Projeção na CPU

A função `project_lla_to_screen` em [mod.rs](file:///c:/Users/rafae/projects/rust/olayer/sdk/native/src/wgpu_cpu_vertex_pipeline/mod.rs) processa a conversão de acordo com o modo de visualização configurado:

### 2.1 Projeção no Modo 3D (Globo Virtual)
1. **Conversão ECEF:** Transforma o ponto LLA $(\phi, \lambda, h)$ em coordenadas retangulares ECEF $(X, Y, Z)$ usando o elipsoide WGS84.
2. **Culling de Oclusão do Horizonte:** Impede a renderização de alvos localizados atrás da curvatura da Terra:
   $$\mathbf{x}_{\text{cam}} \cdot \mathbf{x}_{\text{alvo}} < R_{\text{terra}}^2$$
   Se o produto escalar entre o vetor câmera e o vetor alvo for menor que o quadrado do raio terrestre, o alvo está ocultado pelo horizonte e é descartado.
3. **Multiplicação MVP:** Multiplica as coordenadas ECEF pela matriz View-Projection 3D e converte as coordenadas homogêneas de NDC para coordenadas de tela físicas.

### 2.2 Projeção no Modo 2.5D (Mapa Perspectivo)
Projeta a base do alvo usando a projeção cartográfica plana ativa, adiciona a altitude como o eixo Z, multiplica pela matriz perspectiva da câmera e converte para coordenadas de tela.

### 2.3 Projeção no Modo 2D (Mapa Plano)
Projeta usando as equações geográficas (Estereográfica, LCC, Mercator), rotaciona e dimensiona de acordo com o ângulo de azimute (`bearing`) e o zoom da câmera.

---

## 3. Desenho de Alvos e Vetores Táticos

No loop de eventos desktop:
* **Ícone e Caixa de Seleção:** As aeronaves são desenhadas como círculos no ponto projetado, circundadas por retângulos se selecionadas pelo operador.
* **Vetor de Rumo (Velocity Vector):** Desenha um segmento de reta que representa o deslocamento estimado da aeronave para 1 minuto à frente, calculado via velocidade ($m/s$) e rumo (radianos).
* **Etiqueta de Dados (Label):** Caixa de dados alinhada ao alvo contendo CALLSIGN, Altitude (FL - Flight Level) e Velocidade em nós (KT).

---

## 4. Visualização de Perfil de Voo 2.5D e Alerta CFIT

Quando uma aeronave é selecionada, a SDK ativa a visualização de perfil de voo 2.5D na parte inferior do painel operacional:
1. **Amostragem de Rota:** Gera pontos de rota geodésicos à frente e atrás da posição atual do alvo.
2. **Perfil de Altitude:** O `TerrainEngine` do Core consulta em tempo constante $O(1)$ os arquivos DTED para extrair o relevo do solo sob esses pontos.
3. **Alerta CFIT (Controlled Flight Into Terrain):** Se a diferença entre a altitude da aeronave e a altitude do solo for menor que a margem de segurança tática (ex: 300 metros / 1000 pés), um alerta vermelho com aviso visual `CFIT HAZARD` é disparado na tela do controlador de voo.
