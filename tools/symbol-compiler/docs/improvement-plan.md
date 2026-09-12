# Plano de Melhoria do Symbol Compiler

## Objetivo

Transformar o `tools/symbol-compiler` em uma etapa de build previsível, validada e compatível com os renderers do Olayer, sem introduzir mudanças silenciosas no JSON consumido pelo `DeclarativeProvider`.

O plano prioriza correção e diagnóstico antes da ampliação do subconjunto SVG suportado. A implementação deve manter compatibilidade com os campos atuais:

- `library_name`
- `symbols`
- `bbox`
- `anchor`
- `primitives`
- primitivas `Path`, `Circle` e `Text`

## Problemas atuais

1. A configuração é convertida por cast após `JSON.parse`, sem validação runtime.
2. IDs duplicados sobrescrevem símbolos silenciosamente.
3. Paths SVG são copiados sem normalização, enquanto o renderer TypeScript interpreta apenas parte dos comandos.
4. `transform`, `viewBox`, CSS, `style` e vários atributos SVG são ignorados.
5. O traversal pode processar conteúdo de `<defs>`, `<marker>` e outros elementos não renderizáveis.
6. Erros de XML, configuração e I/O não são classificados nem contextualizados.
7. Os testes são executados manualmente, não há script `test` no pacote e o CI não cobre a ferramenta.
8. O formato de saída não possui versão de schema nem teste de contrato automatizado.

## Princípios de implementação

- Manter o contrato JSON existente até que uma mudança de schema seja aprovada.
- Preferir rejeitar entradas não suportadas a gerar geometria parcialmente incorreta.
- Produzir erros acionáveis, contendo arquivo, símbolo, fase e campo inválido.
- Manter a compilação determinística byte a byte.
- Não calcular automaticamente `bbox` ou `anchor` nesta primeira etapa; esses valores fazem parte da configuração explícita do símbolo.
- Evitar dependências grandes ou um renderer SVG completo quando uma normalização controlada resolver o problema.
- Validar a saída contra o contrato efetivo do Core e o comportamento do SDK.

## Fases

### Fase 0: contrato e baseline

**Objetivo:** registrar o comportamento atual antes de alterar o compilador.

**Tarefas:**

- Documentar o schema atual da configuração e da biblioteca JSON.
- Criar fixtures representando paths, círculos, textos, grupos e estilos atualmente suportados.
- Registrar uma biblioteca compilada de referência para comparação determinística.
- Definir a matriz de suporte SVG: suportado, ignorado com warning ou rejeitado.
- Confirmar os pontos de integração em `core/src/symbol_registry` e `sdk/ts/src/renderer/atlas.ts`.

**Critérios de aceite:**

- Existe uma especificação única para nomes snake_case e tipos numéricos.
- O resultado baseline pode ser reproduzido em execuções consecutivas.
- Cada recurso fora do escopo possui comportamento definido.

### Fase 1: validação e diagnóstico

**Objetivo:** impedir bibliotecas inválidas ou ambíguas.

**Tarefas:**

- Validar `library_name` não vazio.
- Validar `symbols` como array não vazio.
- Validar IDs não vazios, formato permitido e unicidade.
- Validar `svg_path` como string não vazia.
- Validar `bbox` com quatro números finitos e ordem `min <= max`.
- Validar `anchor` com dois números finitos.
- Validar raios, larguras, opacidades, canais RGBA e arrays de tracejado.
- Rejeitar símbolos sem primitivas renderizáveis, salvo uma opção explícita de permitir vazios.
- Substituir `existsSync` por tratamento direto de erros de leitura.
- Criar erros tipados para configuração, parsing SVG, validação e I/O.
- Incluir no erro o caminho da configuração, índice do símbolo e ID.

**Critérios de aceite:**

- Entradas inválidas falham antes da escrita do arquivo de saída.
- IDs duplicados nunca são sobrescritos silenciosamente.
- Valores `NaN`, infinitos, negativos inválidos e alpha fora do intervalo são rejeitados.
- Testes cobrem cada regra de validação e verificam mensagens úteis.

### Fase 2: traversal SVG seguro e estilos

**Objetivo:** tornar a conversão do subconjunto suportado correta e explícita.

**Tarefas:**

- Trocar o traversal genérico por handlers explícitos para `svg`, `g`, `path`, `circle` e `text`.
- Ignorar `<defs>`, `<metadata>`, `<title>`, `<desc>`, `<marker>`, `<clipPath>` e `<mask>` por padrão.
- Definir política para elementos desconhecidos: warning ou erro em modo estrito.
- Implementar a cascata de estilos na ordem documentada: defaults, herança, CSS, atributos e `style` inline.
- Incluir na herança `fill-opacity` e `stroke-opacity`.
- Implementar ou rejeitar explicitamente `display`, `visibility`, `fill-rule`, `stroke-linecap` e `stroke-linejoin`.
- Corrigir a semântica de `none`, especialmente em texto.
- Validar cores hexadecimais, `rgb/rgba` e nomes suportados; emitir erro ou warning para cores desconhecidas.

**Critérios de aceite:**

- Estilos de grupos são propagados de forma consistente.
- Conteúdo de `<defs>` não aparece como primitiva renderizável acidentalmente.
- Todo atributo não suportado é reportado conforme o modo configurado.
- Fixtures com CSS e opacidade herdada produzem saída esperada.

### Fase 3: geometria, paths e transformações

**Objetivo:** eliminar divergências entre o compilador e o renderer TypeScript.

**Decisão recomendada:** manter o JSON atual com `Path.commands`, mas normalizar os comandos para uma gramática mínima que o renderer consiga interpretar corretamente.

**Tarefas:**

- Implementar parser de path com suporte a números, sinais, expoentes e múltiplos pares de coordenadas.
- Converter comandos relativos em coordenadas absolutas.
- Definir tratamento para `M`, `L`, `H`, `V`, `Z`.
- Rejeitar inicialmente `C`, `S`, `Q`, `T` e `A` com erro claro, ou converter curvas/arcos para segmentos somente após medir a perda de qualidade.
- Aplicar uma matriz acumulada para `translate`, `scale`, `rotate` e `matrix` suportados.
- Definir como `viewBox` e dimensões do SVG participam da transformação.
- Atualizar `atlas.ts` para consumir a gramática normalizada sem tratar comandos relativos como absolutos.
- Adicionar testes de múltiplos subpaths, comandos compactados e transformações aninhadas.

**Critérios de aceite:**

- Nenhum path compilado contém comandos que o renderer não suporta.
- Paths relativos e absolutos equivalentes geram a mesma geometria.
- Transformações aninhadas geram coordenadas esperadas.
- Testes do compilador e do atlas validam o mesmo conjunto de fixtures.

### Fase 4: texto e referências

**Objetivo:** melhorar elementos que não podem ser convertidos com segurança pelo parser atual.

**Tarefas:**

- Implementar conteúdo textual simples e definir política para `<tspan>`.
- Preservar ou normalizar whitespace conforme opção documentada.
- Validar unidades de `font-size` e posicionamento `x/y`.
- Definir suporte para `text-anchor`, peso e família de fonte ou rejeitar essas propriedades.
- Avaliar suporte a `<use>` com resolução restrita de IDs locais.
- Proibir referências externas e ciclos de referência.

**Critérios de aceite:**

- Texto invisível não vira texto preto opaco por fallback implícito.
- Conteúdo não suportado falha ou gera warning determinístico.
- Referências não confiáveis não permitem leitura de arquivos externos.

### Fase 5: CLI, reprodutibilidade e segurança operacional

**Objetivo:** tornar a ferramenta adequada para CI e pipelines de build.

**Tarefas:**

- Adicionar opções `--strict`, `--verbose` e, se necessário, `--quiet`.
- Retornar códigos de saída diferentes para uso, configuração, parsing e I/O.
- Escrever a saída em arquivo temporário e renomear atomicamente.
- Adicionar newline final e serialização estável.
- Definir ordenação de símbolos ou preservar explicitamente a ordem documentada.
- Adicionar limites configuráveis para tamanho de configuração, tamanho de SVG, número de símbolos, número de nós e profundidade.
- Validar caminhos relativos ao diretório da configuração; documentar o tratamento de caminhos absolutos.
- Configurar o parser XML para rejeitar entidades externas e construções não necessárias.
- Centralizar a versão da CLI e do schema, evitando `0.1.0` duplicado em código e `package.json`.

**Critérios de aceite:**

- Falhas nunca deixam um arquivo final parcialmente escrito.
- A mesma entrada produz o mesmo arquivo em execuções repetidas.
- A CLI fornece diagnóstico suficiente para uso em CI sem stack trace por padrão.
- Fixtures acima dos limites falham de forma controlada.

### Fase 6: testes, documentação e CI

**Objetivo:** impedir regressões e manter o contrato verificável.

**Tarefas:**

- Migrar `src/test.ts` para um runner consistente, preferencialmente Vitest ou `node:test`.
- Adicionar scripts `test`, `test:watch` e, se aplicável, `lint` ao `package.json`.
- Adicionar testes unitários para cores, estilos, validação, traversal, paths, transforms, texto e CLI.
- Adicionar testes de golden files para JSON determinístico.
- Adicionar teste de contrato que carregue a saída no `DeclarativeProvider` Rust.
- Adicionar teste de renderização compartilhado entre compiler e `atlas.ts`.
- Executar `npm run build` e os testes do pacote no CI.
- Atualizar o README com limitações, modos estrito/normal e exemplos de falha.
- Corrigir a documentação geral para refletir `library_name`, `dash_array`, `offset_x`, `offset_y` e `font_size`.

**Critérios de aceite:**

- O pacote possui testes executáveis por comando documentado.
- O CI falha quando o compilador não compila, os testes falham ou o golden output muda sem atualização explícita.
- A documentação descreve exatamente o subset SVG suportado.

## Escopo fora da primeira entrega

- Suporte completo a todo o padrão SVG.
- Rasterização de filtros, máscaras, gradientes e efeitos.
- Conversão automática de qualquer curva ou arco sem avaliação visual.
- Cálculo automático de `bbox` e `anchor` a partir da geometria.
- Alteração imediata do schema Rust para representar novas primitivas.

## Ordem de implementação

1. Fase 0: contrato e fixtures.
2. Fase 1: validação e erros.
3. Fase 2: traversal e estilos.
4. Fase 3: paths, transformações e alinhamento com o atlas.
5. Fase 5: CLI, determinismo e limites.
6. Fase 6: testes, documentação e CI.
7. Fase 4: texto avançado e referências, conforme necessidade dos símbolos reais.

A Fase 4 pode ser antecipada se os assets existentes exigirem `<tspan>` ou `<use>`. Ela não deve bloquear as garantias de validação e compatibilidade das fases anteriores.

## Definição de pronto

O trabalho será considerado concluído quando:

- configurações inválidas forem rejeitadas com mensagens contextualizadas;
- a saída passar pelo `DeclarativeProvider` sem incompatibilidades;
- paths compilados forem totalmente compreendidos pelo renderer TypeScript;
- recursos SVG não suportados forem rejeitados ou sinalizados de forma explícita;
- a saída for determinística e escrita atomicamente;
- testes do pacote e do contrato forem executados no CI;
- README e documentação do projeto refletirem o comportamento real.

## Comandos de verificação previstos

```bash
cd tools/symbol-compiler
npm install
npm run build
npm test
```

Na integração do repositório:

```bash
cargo test --workspace --exclude olayer-desktop-demo
cd sdk/ts
npm run test:run
```
