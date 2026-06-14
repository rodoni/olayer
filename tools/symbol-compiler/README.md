# Olayer Symbol Compiler

Olayer Symbol Compiler é uma ferramenta de linha de comando (CLI) utilitária em Node.js/TypeScript projetada para compilar arquivos vetoriais **SVG** de símbolos em uma única biblioteca declarativa consolidada em formato **JSON**.

Este formato JSON é compatível com o componente `DeclarativeProvider` do **Olayer Core** escrito em Rust/WASM, permitindo o carregamento e estilização dinâmica de símbolos em tempo de execução sem inchar o núcleo WebAssembly com decodificadores complexos.

## Recursos Suportados

- **Elementos SVG:** Conversão automática de elementos `<path>`, `<circle>` e `<text>` em primitivas do Olayer.
- **Estilos:** Processamento de cores CSS (hexadecimais, RGB/RGBA e nomes padrão), largura de contorno (`stroke-width`), tracejados (`stroke-dasharray`) e opacidades (`opacity`, `fill-opacity`, `stroke-opacity`).
- **Nesting:** Suporte a agrupamentos `<g>` herdando atributos de estilização e opacidades acumuladas de forma recursiva.

---

## Como Usar

### 1. Instalação e Build

Navegue para a pasta da ferramenta e compile o código TypeScript:

```bash
cd tools/symbol-compiler
npm install
npm run build
```

### 2. Formato do Arquivo de Configuração

Crie um arquivo JSON de configuração (ex: `symbols.config.json`) mapeando os IDs de símbolos desejados para os caminhos relativos de seus respectivos arquivos SVG:

```json
{
  "library_name": "OlayerAviationSymbols",
  "symbols": [
    {
      "id": "civil:plane",
      "svg_path": "./plane.svg",
      "bbox": [-12.0, -12.0, 12.0, 12.0],
      "anchor": [0.0, 0.0]
    },
    {
      "id": "mil:fighter",
      "svg_path": "./fighter.svg",
      "bbox": [-12.0, -12.0, 12.0, 12.0],
      "anchor": [0.0, 0.0]
    }
  ]
}
```

- **`id`:** O identificador exclusivo do símbolo no `SymbolRegistry`.
- **`svg_path`:** Caminho relativo ao arquivo de configuração para o asset SVG.
- **`bbox`:** Caixa delimitadora do símbolo: `[min_x, min_y, max_x, max_y]`.
- **`anchor`:** Ponto de ancoragem (ponto central ou de rotação) do símbolo: `[x, y]`.

### 3. Execução do Compilador

Execute a ferramenta passando o arquivo de configuração e o arquivo JSON de saída desejado:

```bash
node dist/cli.js -c path/to/symbols.config.json -o path/to/compiled_symbols.json
```

---

## Integração com Olayer TS SDK

Para usar a biblioteca compilada no aplicativo host, faça o fetch do arquivo JSON compilado e registre-o no `symbolRegistry` do controlador:

```typescript
const response = await fetch("./assets/compiled_symbols.json");
const symbolsJson = await response.text();

controller.symbolRegistry.register_declarative_provider(symbolsJson);
```
