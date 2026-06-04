# Arquitetura do Componente: Target Interpolator (`core::interpolator`)

Este documento detalha o design técnico, as equações cinemáticas geodésicas e as estruturas de dados do componente **Target Interpolator** do Olayer Core. Este módulo é responsável por sincronizar e prever a trajetória tridimensional de alvos dinâmicos geodésicos (aeronaves, veículos terrestres, embarcações, UAVs, etc.) em tempo de execução usando *Dead Reckoning*.

---

## 1. Responsabilidades

O **Target Interpolator** opera como um motor de predição cinemática passiva de alto desempenho, encarregado de:
1. **Rastreamento de Estados de Alvos:** Armazenar uma tabela indexada em memória com os estados físicos reais (`TargetState`) relatados periodicamente pelos sensores (radar, ADS-B, GPS) para cada alvo tático.
2. **Predição Cinemática 3D Geodésica (*Dead Reckoning*):** Calcular a posição tridimensional $(\phi, \lambda, h)$ estimada no globo a partir do tempo decorrido desde o último ping do sensor, utilizando o modelo elipsoidal do WGS84.
3. **Desacoplamento de Projeção:** Manter a estimativa física dos alvos estritamente no espaço geodésico, livre de coordenadas ou limites de tela 2D. A projeção e translação final de tela são de responsabilidade da SDK cliente, que consome os dados e aplica a projeção cartográfica ativa do Olayer Core.
4. **Atualizações Assíncronas:** Lidar de forma transparente com atualizações de sensores recebidas em taxas de frequência variáveis e baixas (ex: ~1 Hz) e interpolá-las continuamente para a taxa de quadros de exibição do cliente (15 a 60 FPS).

---

## 2. Diagrama de Estruturas e Relacionamento

```mermaid
classDiagram
    direction TB

    class InterpolationEngine {
        -targets: HashMap~String, TargetState~
        +new() InterpolationEngine
        +update_target(state: TargetState) Result~() , InterpolatorError~
        +remove_target(id: &str) bool
        +interpolate_all(current_time: f64) Result~Vec~InterpolatedTarget~, InterpolatorError~
    }

    class TargetState {
        +id: String
        +last_position: LatLon
        +speed_mps: f64
        +track_heading_rad: f64
        +vertical_rate_mps: f64
        +last_ping_time: f64
        +validate() Result~() , InterpolatorError~
    }

    class InterpolatedTarget {
        +id: String
        +position: LatLon
        +heading_rad: f64
    }

    class InterpolatorError {
        <<enumeration>>
        InvalidState(String)
        NegativeTimeDelta(String)
        GeodesyFailure(GeodesyError)
    }

    %% Relações
    InterpolationEngine "1" *-- "*" TargetState : gerencia
    InterpolationEngine ..> InterpolatedTarget : computa
    TargetState "1" *-- "1" LatLon : posicionada em
    InterpolatedTarget "1" *-- "1" LatLon : posicionada em
```

---

## 3. Estrutura Física do Módulo (`core/src/interpolator`)

A organização física das fontes em Rust segue o padrão modular do framework:

```text
core/src/interpolator/
├── mod.rs               # Facade do módulo (Re-exports)
├── errors.rs            # Enum de erros (InterpolatorError)
├── state.rs             # Estruturas TargetState e InterpolatedTarget
├── engine.rs            # Lógica do InterpolationEngine
└── tests.rs             # Testes de cinemática e extrapolação
```

---

## 4. Formulação Matemática do *Dead Reckoning*

A cada atualização ou solicitação de frame, o motor de interpolação estima a nova coordenada para o timestamp do sistema $t_{\text{current}}$ com base no timestamp do sensor $t_{\text{last\_ping}}$:

### 4.1 Delta de Tempo ($dt$)
$$dt = t_{\text{current}} - t_{\text{last\_ping}}$$
*Se $dt < 0$, o alvo correspondente é ignorado e omitido da resposta daquele frame para evitar que desvios temporais de um único sensor interfiram no restante do lote de alvos (clock skew).*

### 4.2 Translação Horizontal Geodésica
A movimentação horizontal do alvo sobre o elipsoide WGS84 é obtida resolvendo o **Problema Geodésico Direto**:
1. Distância horizontal percorrida:
   $$d = v_{\text{horizontal}} \times dt$$
2. O ponto de origem $p_0 = (\phi_{\text{last}}, \lambda_{\text{last}})$, o rumo/azimute inicial $\alpha = \psi_{\text{inicial}}$, e a distância $d$ são passados ao **Vincenty Solver** (ou Haversine Solver em caso de fallback) da `Geodesy Engine`:
   $$p_{\text{interpolated}} = \text{direct}(p_0, \alpha, d, \text{WGS84})$$
   $$\phi_{\text{new}} = p_{\text{interpolated}}.\phi, \quad \lambda_{\text{new}} = p_{\text{interpolated}}.\lambda$$

### 4.3 Variação Vertical de Altitude
A altitude acima do elipsoide ($h$) é extrapolada linearmente pela taxa de subida ou descida vertical:
$$h_{\text{new}} = h_{\text{last}} + (v_{\text{vertical}} \times dt)$$

### 4.4 Rumo Interpolado
Em manobras lineares simples, o rumo é assumido constante:
$$\psi_{\text{new}} = \psi_{\text{inicial}}$$

---

## 5. Critérios de Performance e Robusteza

1. **Evitar Alocações no Heap em Loop:** A lista retornada por `interpolate_all` é pré-alocada com capacidade baseada no número de alvos ativos (`Vec::with_capacity(self.targets.len())`) para eliminar alocações redundantes a cada frame.
2. **Tolerância a Atrasos de Sensor:** Se um alvo não receber pings por um intervalo longo (ex: $dt > 30.0\text{ segundos}$), a engine pode suspendê-lo do cálculo dinâmico (*stale target*) para evitar extrapolações físicas irreais.
3. **Precisão e Velocidade:** Para alvos em altas velocidades (aeronaves supersônicas/caças), o uso do resolvedor preciso de Vincenty garante que a extrapolação siga trajetórias reais de grande círculo, evitando desvios significativos presentes em aproximações planas simplificadas.
