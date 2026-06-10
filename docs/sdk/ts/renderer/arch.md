# Componente SDK TS: Render Pipelines & Texture Atlas (`sdk/ts/src/renderer`)

A camada de **Renderização** da SDK TypeScript é encarregada de projetar e desenhar todos os dados matemáticos e táticos em tela (WebGL/GPU para o terreno e fundo de mapa; Canvas 2D/CPU para os alvos e etiquetas). O **Texture Atlas** centraliza símbolos em buffers unificados na GPU para maximizar a performance.

---

## 1. Responsabilidades
* **GPU Render Pipeline (`WebGLRenderer`):**
  * Compilar Shaders nativos WebGL/WebGPU.
  * Upload de dados e posicionamento geográfico do Grid cartográfico.
  * Vincular matrizes de Projeção-Visualização $4 \times 4$ e desenhar malhas tridimensionais (como elipsoides do globo 3D).
* **CPU Render Pipeline (`CPURenderer`):**
  * Desenhar alvos, blocos de dados (data blocks) e vetores de rumo em tempo real (60 FPS).
  * Projetar coordenadas tridimensionais WGS84 para coordenadas de tela $(X,Y)$ usando a matriz de transformação do Olayer Controller.
  * Executar algoritmos de **Anti-cluttering (anti-sobreposição)** para manter etiquetas legíveis.
* **Texture Atlas Manager (`TextureAtlasManager`):**
  * Centralizar todos os símbolos (procedurais, SVG ou PNG) em uma única textura GPU compartilhada.
  * Renderizar símbolos usando billboards dinâmicos (placas planas orientadas de frente para a câmera), garantindo legibilidade em 3D.
  * Executar desenho instanciado (`drawElementsInstanced`) para plotar milhares de aeronaves em uma única draw call.

---

## 2. Detalhes de Implementação e Algoritmos

### 2.1 Prevenção de Sobreposição de Etiquetas (Anti-cluttering)
Para que telas operacionais de controle de tráfego aéreo mantenham legibilidade em alta densidade de tráfego:
1. **Fração de Projeção:** Converte posições geodésicas interpoladas 3D (WGS84 `LatLon`) dos alvos de radar em coordenadas de tela $(X,Y)$.
2. **Bounding Box das Etiquetas:** Calcula o retângulo do bloco de dados (tamanho de texto + velocidade + rumo).
3. **Mapeamento de Ocupação:** Uma árvore estática 2D ou tabela de colisão em tela (*Grid Collision Table*) armazena retângulos de alvos prioritários.
4. **Resolução de Conflitos (Offset Alternado):** Se houver conflito, a etiqueta tenta orbitar em torno do símbolo em posições de bússola pré-definidas (Nordeste -> Sudeste -> Sudoeste -> Noroeste). Se nenhuma servir, a etiqueta secundária é temporariamente ocultada.

---

## 3. Interfaces e Estrutura de Classes

### 3.1 WebGLRenderer
```typescript
export class WebGLRenderer {
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram | null = null;
  private gridBuffer: WebGLBuffer | null = null;
  private gridLineCount = 0;

  constructor(gl: WebGL2RenderingContext);
  public rebuildGrid(projection: any, viewMode: string): void;
  public renderGrid(viewProjMatrix: Float32Array): void;
  public destroy(): void;
}
```

### 3.2 TextureAtlasManager
```typescript
export class TextureAtlasManager {
  private gl: WebGL2RenderingContext;
  private texture: WebGLTexture | null = null;
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;

  constructor(gl: WebGL2RenderingContext);
  public registerSymbol(id: string, drawFn: (ctx: CanvasRenderingContext2D) => void, width: number, height: number): void;
  public getTexture(): WebGLTexture | null;
  public destroy(): void;
}
```

---

## 4. Gerenciamento de Memória & Ciclo de Vida (ADR-004)

Como o WebAssembly opera em uma máquina virtual com memória linear isolada e sem monitoramento do Garbage Collector (GC), a SDK TS implementa a liberação estrita de recursos Rust:

```typescript
export class OlayerController {
  // ...
  
  /**
   * Destrutor explícito que deve ser invocado pela aplicação Host
   * ao desmontar o componente de mapa.
   */
  public destroy(): void {
    this.stopLoop();
    this.dataManager.clearCache();
    
    // Desalocação explícita na Heap do WebAssembly
    this.terrainEngine.free();
    this.interpolator.free();
    this.projection.free();
  }
}
```
O descarte explícito previne memory leaks que poderiam comprometer a estabilidade do console ATC a longo prazo.
