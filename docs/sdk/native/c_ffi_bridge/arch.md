# Arquitetura: Bridge C-FFI (Nativo)

Este documento detalha o design arquitetural, a especificação técnica, as diretrizes de compatibilidade binária e o gerenciamento de memória da ponte de interoperabilidade **C-FFI** do Olayer, localizada em [c_ffi_bridge](file:///c:/Users/rafae/projects/rust/olayer/sdk/native/c_ffi_bridge).

Este componente expõe as capacidades geométricas de missão crítica do motor em Rust para aplicativos locais escritos em linguagens nativas compiladas (como C, C++, C# ou Go).

---

## 1. Diagrama de Integração C-FFI (C4 Model - Nível 3)

A ponte C-FFI opera no nível da ABI (Application Binary Interface) do sistema operacional, expondo símbolos exportados através de convenções de chamada padrão C (`extern "C"`).

```mermaid
graph TB
    classDef native fill:#C8E6C9,stroke:#388E3C,color:#1B5E20,stroke-width:2px;
    classDef ffi fill:#A5D6A7,stroke:#2E7D32,color:#1B5E20,stroke-width:2px;
    classDef core fill:#E1F5FE,stroke:#0288D1,color:#01579B,stroke-width:2px;

    subgraph Native_Client_Environment ["Ambiente do Aplicativo Host (C++ / C)"]
        host_app["🖥️ Aplicativo Host ATC Nativo<br>[C++ executable]"]:::native
        raw_buffers["💾 Buffers Brutos de Memória<br>(DTED do disco local)"]:::native
        lib_h["📄 Header libolayer_native.h<br>(Gerado pelo cbindgen)"]:::native
    end

    subgraph C_FFI_Bridge_Boundary ["C-FFI Bridge (Rust ABI-C Wrapper)"]
        ffi_symbols["🔗 Símbolos Exportados C-FFI<br>(#[no_mangle] extern C)"]:::ffi
        unsafe_translator["⚙️ Unsafe Marshalling<br>(Conversão de Pointers para slices)"]:::ffi
    end

    subgraph Rust_Core_Engine ["Olayer Core (Rust)"]
        geodesy["📐 Geodesy Module"]:::core
        projections["🗺️ Projections Module"]:::core
        terrain["⛰️ Terrain Engine"]:::core
        interpolator["⏱️ Target Interpolator"]:::core
    end

    %% Integração e Compilação
    lib_h -.->|Declara assinaturas e tipos| host_app
    host_app -->|1. Chama funções exportadas| ffi_symbols
    raw_buffers -->|2. Passa ponteiros brutos de bytes| ffi_symbols
    ffi_symbols -->|3. Valida e converte ponteiros| unsafe_translator

    %% Encaminhamento para o Core
    unsafe_translator -->|4. Delega cálculos geodesia| geodesy
    unsafe_translator -->|5. Projeta coordenadas/matrizes| projections
    unsafe_translator -->|6. Carrega/Consulta terreno| terrain
    unsafe_translator -->|7. Atualiza/Interpola alvos| interpolator

    linkStyle 1,2,3,4,5,6,7 stroke:#555,stroke-width:1.5px;
```

---

## 2. Responsabilidades

O componente **C-FFI Bridge** possui as seguintes atribuições principais:
1. **Compatibilidade com a ABI C:** Garantir que todas as funções exportadas utilizem a convenção de chamada de sistema padrão (`extern "C"`) e que as structs tenham layout compatível (`#[repr(C)]`).
2. **Construção de DLLs e Bibliotecas Estáticas:** Compilar o código Rust como biblioteca dinâmica (`.dll`/`.so`/`.dylib`) ou estática (`.lib`/`.a`).
3. **Autogeração de Cabeçalhos (Header Generator):** Utilizar a crate `cbindgen` para gerar automaticamente o arquivo de declarações C/C++ `libolayer_native.h`.
4. **Gerenciamento Seguro de Ponteiros (Pointers Translators):** Converter ponteiros brutos C (`*const u8`, `*const c_char`, etc.) em slices (`&[u8]`) e strings Rust de forma segura dentro de escopos `unsafe`.
5. **Tratamento de Exceções e Código de Erro:** Capturar possíveis pânicos (*unwind panics*) no Rust para impedir que derrubem o processo do host C++ (usando `std::panic::catch_unwind`), retornando códigos de erro estruturados (`int32_t`).

---

## 3. Interfaces Projetadas (C-API Exports)

Para permitir a integração com C/C++, as assinaturas de dados expõem ponteiros opacos para as structs de controle do Rust. As implementações completas e de baixo nível encontram-se em [c_ffi_bridge/src/lib.rs](file:///c:/Users/rafae/projects/rust/olayer/sdk/native/c_ffi_bridge/src/lib.rs).

### 3.1 Definições C-Compatíveis
```rust
use std::os::raw::{c_char, c_int};
use olayer_core::geodesy::LatLon;
use olayer_core::terrain::TerrainEngine;
use olayer_core::interpolator::InterpolationEngine;

/// C representation of a geodetic coordinate.
#[repr(C)]
pub struct C_LatLon {
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
}

/// C representation of an interpolated target.
#[repr(C)]
pub struct C_InterpolatedTarget {
    pub id: *mut c_char,
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
    pub heading_rad: f64,
}

/// C representation of a vertical profile point.
#[repr(C)]
pub struct C_ProfilePoint {
    pub distance_meters: f64,
    pub ground_elevation: f64,
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
}
```

### 3.2 Funções da API do Terreno (DTED)
* `olayer_terrain_engine_create`: Instancia o motor de terreno no Heap do Rust e retorna um ponteiro opaco (`*mut TerrainEngine`).
* `olayer_terrain_engine_load_tile`: Analisa e registra um buffer binário bruto DTED em memória. Escreve as coordenadas de origem nos ponteiros passados.
* `olayer_terrain_engine_unload_tile`: Remove uma célula de terreno da memória pelo seu grau de coordenadas.
* `olayer_terrain_engine_get_elevation`: Consulta e retorna a altitude do solo no ponto geográfico especificado em tempo constante $O(1)$.
* `olayer_terrain_engine_get_vertical_profile`: Calcula o perfil vertical de terreno sob a rota fornecida.
* `olayer_profile_points_free`: Libera a memória do array de pontos de perfil alocados pelo Rust.
* `olayer_terrain_engine_free`: Destrói com segurança a instância do motor de terreno.

### 3.3 Funções da API do Interpolador de Radar (MSAW e Dead Reckoning)
* `olayer_interpolator_create`: Instancia o motor de interpolação e retorna um ponteiro opaco.
* `olayer_interpolator_create_with_threshold`: Instancia com um tempo limite customizado antes de expirar estados obsoletos.
* `olayer_interpolator_update`: Atualiza ou insere o vetor cinemático de estado de uma aeronave.
* `olayer_interpolator_remove`: Remove um alvo do monitoramento por ID.
* `olayer_interpolator_interpolate_all`: Calcula o posicionamento físico de todos os alvos ativos para o tempo atual do sistema usando *Dead Reckoning*.
* `olayer_interpolated_targets_free`: Desaloca com segurança o array de alvos retornados pelo interpolador Rust (incluindo as strings C-compatíveis embutidas).
* `olayer_interpolator_free`: Desaloca a instância do motor de interpolação.

---

## 4. Tratamento de Exceções e Segurança de Pânico

Se um pânico em Rust tentar desenrolar a pilha (*unwinding*) cruzando a fronteira FFI para o código C/C++, isso resultará em comportamento indefinido (Undefined Behavior) e crash iminente da aplicação host.

Para mitigar isso, cada chamada exposta na ponte utiliza a função `std::panic::catch_unwind` para capturar falhas e mapeá-las em retornos numéricos seguros:
```rust
let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    engine_ref.load_tile(data_slice)
}));

match result {
    Ok(Ok(key)) => 0, // Sucesso
    Ok(Err(_)) => -2,  // Erro mapeado logicamente
    Err(_) => -99,    // Pânico capturado e contido com segurança
}
```

---

## 5. Gerenciamento de Memória Nativa e Segurança (Ownership Rules)

A integração via FFI nativo exige regras rígidas sobre quem é o "dono" (owner) de cada recurso alocado para evitar vazamentos de memória e falhas de segmentação (*Segmentation Faults*).

### 5.1 Fronteiras de Propriedade da Memória (Ownership Boundary)
* **Regra Geral:** O lado da fronteira FFI que aloca a memória deve ser obrigatoriamente o mesmo que a libera.
* **Dados Alocados pelo Rust:** Quando a ponte Rust aloca um objeto no Heap (ex: `olayer_terrain_engine_create`), o host em C++ recebe um ponteiro bruto (`*mut TerrainEngine`). O host C++ **nunca** deve desalocar este ponteiro chamando `free()` do C ou `delete` do C++. Em vez disso, ele deve chamar o destrutor exportado correspondente (ex: `olayer_terrain_engine_free`).
* **Dados Alocados pelo Host C++:** Buffers binários de arquivos DTED carregados pelo C++ em memória do sistema são passados ao Rust via ponteiro simples (`*const u8`). O Rust acessa esses bytes estritamente para leitura e **não tenta** liberar ou manter a propriedade do ponteiro original. A responsabilidade por desalocar o buffer de arquivos após a conclusão da leitura permanece 100% no host C++.

### 5.2 Segurança Concorrente (Thread-Safety)
* O Olayer Core é desenhado para ser thread-safe (estruturas implementam `Send` e `Sync` no Rust).
* Ponteiros retornados de construtores (ex: `*mut TerrainEngine`) podem ser compartilhados entre diferentes threads de execução do aplicativo host (como uma thread de processamento tático de radar e uma thread de renderização da interface local wgpu/Vulkan).
* **Importante:** A segurança concorrente mútua é garantida porque os dados geográficos e modelos matemáticos de leitura (como `TerrainEngine` carregado) realizam apenas leituras simultâneas sem estado interno mutável concorrente. Se modificações dinâmicas (gravações de novos tiles de terreno) ocorrerem em paralelo a leituras, o host C++ deve sincronizar o acesso a esses ponteiros usando travas nativas (`std::mutex` ou equivalentes).
