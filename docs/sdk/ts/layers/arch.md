# Componente SDK TS: Layer Manager (`sdk/ts/src/layers`)

O **Layer Manager** gerencia a pilha de camadas (Layer Stack) visuais, definindo a ordem de desenho (*z-index*), visibilidade, opacidade e otimização de repintura por meio de pipelines de renderização segregados.

---

## 1. Responsabilidades
* **Composição de Pilha:** Organizar camadas estáticas (mapa base, fronteiras, aerovias) e dinâmicas (radar meteorológico, tráfego aéreo, anéis de distância).
* **Segregação de Repintura (Otimização):**
  * **Pintura Estática (WebGL):** Avaliada apenas sob interações físicas de câmera (Pan, Zoom, Rotação), salvando resultados em buffers estáticos da GPU.
  * **Pintura Dinâmica (Canvas 2D):** Desenhada em tempo real em cada frame (até 60 FPS) por cima do plano de fundo estático, sem custo de reprocessamento do fundo do mapa.
* **Ciclo de Vida de Visualização:** Encapsular e disparar os gatilhos de renderização das camadas filhas de forma ordenada.

---

## 2. Interfaces e Estrutura de Classes

```typescript
/**
 * Interface abstrata para todas as camadas do Olayer.
 */
export abstract class Layer {
  public id: string;
  public visible: boolean = true;
  public opacity: number = 1.0;

  constructor(id: string) {
    this.id = id;
  }

  /**
   * Chamado para renderizar elementos estáticos que residem na GPU (WebGL/WebGPU).
   * Só é acionado quando a câmera muda ou o mapa é atualizado.
   */
  public abstract renderStatic(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void;

  /**
   * Chamado para desenhar sobreposições dinâmicas rápidas usando o contexto Canvas 2D.
   * Acionado em todo frame tático ativo (até 60 FPS).
   */
  public abstract renderDynamic(ctx: CanvasRenderingContext2D, currentTime: number): void;
}

/**
 * Coordenador da pilha de camadas visuais do Olayer.
 */
export class LayerManager {
  private layers: Layer[] = [];

  /**
   * Insere uma nova camada na pilha de visualização.
   */
  public addLayer(layer: Layer): void;

  /**
   * Remove uma camada pelo ID.
   */
  public removeLayer(id: string): boolean;

  /**
   * Reordena o posicionamento relativo de uma camada na pilha.
   */
  public reorderLayer(id: string, newIndex: number): void;

  /**
   * Retorna todas as camadas carregadas.
   */
  public getLayers(): Layer[];

  /**
   * Varre e renderiza camadas estáticas WebGL visíveis.
   */
  public renderStaticLayers(gl: WebGL2RenderingContext, viewProjMatrix: Float32Array): void;

  /**
   * Varre e renderiza camadas dinâmicas Canvas 2D visíveis.
   */
  public renderDynamicLayers(ctx: CanvasRenderingContext2D, currentTime: number): void;
}
```
