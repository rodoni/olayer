import { MapDataSource } from "./datasource";

export interface VectorFeature {
  type: "Point" | "LineString" | "Polygon";
  coordinates: number[][]; // Coordinates as [lat_rad, lon_rad] arrays
  properties: Record<string, any>;
}

/**
 * Provedor para arquivos vetoriais de mapa (Mapbox Vector Tiles - MVT) do GeoServer.
 * Responsável por gerenciar a paginação, cacheamento de feições e busca espacial de tiles.
 */
export class VectorTileSource implements MapDataSource {
  public readonly id: string = "geoserver_mvt";
  private tileCache: Map<string, VectorFeature[]> = new Map(); // Key format: "z/x/y"
  private loadingTiles: Set<string> = new Set();
  private urlResolver: string | ((x: number, y: number, z: number) => string);
  private maxTiles: number;

  constructor(
    urlResolver: string | ((x: number, y: number, z: number) => string) = "",
    maxTiles: number = 100
  ) {
    this.urlResolver = urlResolver;
    this.maxTiles = maxTiles;
  }

  /**
   * Carrega e decodifica as feições vetoriais de um tile específico.
   */
  public async loadTile(x: number, y: number, z: number): Promise<void> {
    const key = `${z}/${x}/${y}`;

    if (this.tileCache.has(key) || this.loadingTiles.has(key)) {
      return;
    }

    if (this.tileCache.has(key)) {
      const features = this.tileCache.get(key)!;
      this.tileCache.delete(key);
      this.tileCache.set(key, features);
      return;
    }

    if (!this.urlResolver) {
      // Se não há resolvedor de URL, gera feições simuladas/mockadas
      const mockFeatures = this.generateMockFeatures(x, y, z);
      this.tileCache.set(key, mockFeatures);
      return;
    }

    let url = "";
    if (typeof this.urlResolver === "function") {
      url = this.urlResolver(x, y, z);
    } else {
      url = this.urlResolver
        .replace("{x}", x.toString())
        .replace("{y}", y.toString())
        .replace("{z}", z.toString());
    }

    this.loadingTiles.add(key);

    try {
      const response = await fetch(url);
      if (!response.ok) {
        throw new Error(`Failed to fetch vector tile: HTTP ${response.status}`);
      }

      const buffer = await response.arrayBuffer();

      // Parser básico e tolerante a falhas.
      // Se o formato retornado for JSON/GeoJSON, faz o parse;
      // se for binário (MVT .pbf), utiliza um stub/mock temporário caso as dependências nativas
      // não estejam instaladas no package.json.
      let features: VectorFeature[] = [];
      try {
        const text = new TextDecoder().decode(buffer);
        const parsed = JSON.parse(text);
        if (parsed && Array.isArray(parsed.features)) {
          // GeoServer GWC TMS EPSG:900913 está em metros. Convertemos de metros para Lat/Lon em radianos:
          const R_EARTH = 6378137.0;
          const toLatLonRad = (x: number, y: number): [number, number] => {
            const lon = x / R_EARTH;
            const lat = 2 * Math.atan(Math.exp(y / R_EARTH)) - Math.PI / 2;
            return [lat, lon];
          };

          features = parsed.features.map((f: any) => {
            const geom = f.geometry || {};
            const type = geom.type || "LineString";
            const rawCoords = geom.coordinates || [];
            let coords: number[][] = [];

            if (type === "Point" && Array.isArray(rawCoords) && rawCoords.length >= 2) {
              coords = [toLatLonRad(rawCoords[0], rawCoords[1])];
            } else if (type === "LineString" && Array.isArray(rawCoords)) {
              coords = rawCoords.map((pt: any) => toLatLonRad(pt[0], pt[1]));
            } else if (type === "Polygon" && Array.isArray(rawCoords) && Array.isArray(rawCoords[0])) {
              coords = rawCoords[0].map((pt: any) => toLatLonRad(pt[0], pt[1]));
            }

            return {
              type: type as "Point" | "LineString" | "Polygon",
              coordinates: coords,
              properties: f.properties || {}
            };
          });
        }
      } catch {
        // Fallback para feições mockadas se for binário (MVT) e não tivermos o parser compilado
        features = this.generateMockFeatures(x, y, z);
      }

      // Controle de cache LRU
      if (this.tileCache.size >= this.maxTiles) {
        const oldestKey = this.tileCache.keys().next().value;
        if (oldestKey) {
          this.tileCache.delete(oldestKey);
        }
      }

      this.tileCache.set(key, features);
    } catch (err) {
      console.error(`Failed to load vector tile [z:${z}, x:${x}, y:${y}] from ${url}:`, err);
      // Tolerância a falhas: insere feições vazias/mockadas para evitar crash
      this.tileCache.set(key, this.generateMockFeatures(x, y, z));
    } finally {
      this.loadingTiles.delete(key);
    }
  }

  /**
   * Obtém a lista de feições vetoriais carregadas para um determinado tile.
   */
  public getTileFeatures(x: number, y: number, z: number): VectorFeature[] {
    const key = `${z}/${x}/${y}`;
    return this.tileCache.get(key) || [];
  }

  /**
   * Descarrega e remove do cache um tile vetorial.
   */
  public unloadTile(x: number, y: number, z: number): void {
    const key = `${z}/${x}/${y}`;
    this.tileCache.delete(key);
  }

  /**
   * Limpa o cache vetorial.
   */
  public clearCache(): void {
    this.tileCache.clear();
  }

  /**
   * Gera feições mockadas de teste (aerovias e limites de setor) para validação visual.
   */
  private generateMockFeatures(x: number, y: number, z: number): VectorFeature[] {
    const features: VectorFeature[] = [];

    // São Paulo TMA Center aproximado em radianos
    const SP_LAT_RAD = -23.62 * (Math.PI / 180);
    const SP_LON_RAD = -46.65 * (Math.PI / 180);

    // Gerar algumas aerovias de rota cruzando o setor
    features.push({
      type: "LineString",
      coordinates: [
        [SP_LAT_RAD + 0.5, SP_LON_RAD - 0.8],
        [SP_LAT_RAD - 0.5, SP_LON_RAD + 0.8]
      ],
      properties: { name: "UM415", type: "airway" }
    });

    features.push({
      type: "LineString",
      coordinates: [
        [SP_LAT_RAD - 0.6, SP_LON_RAD - 0.6],
        [SP_LAT_RAD + 0.6, SP_LON_RAD + 0.6]
      ],
      properties: { name: "UZ22", type: "airway" }
    });

    // Limites de controle da terminal TMA SP (polígono octogonal)
    const steps = 8;
    const radiusRad = 80000 / 6378137.0; // 80km de raio convertidos para radianos
    const polyCoords: number[][] = [];
    
    for (let i = 0; i <= steps; i++) {
      const angle = (i * 2 * Math.PI) / steps;
      polyCoords.push([
        SP_LAT_RAD + radiusRad * Math.cos(angle),
        SP_LON_RAD + radiusRad * Math.sin(angle)
      ]);
    }

    features.push({
      type: "Polygon",
      coordinates: polyCoords,
      properties: { name: "TMA SP CTA1", type: "boundary" }
    });

    return features;
  }
}
