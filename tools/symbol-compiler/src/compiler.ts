import * as fs from 'fs';
import * as path from 'path';
import { XMLParser } from 'fast-xml-parser';

export interface Color {
  r: number;
  g: number;
  b: number;
  a: number;
}

export interface Stroke {
  color: Color;
  width: number;
  dash_array?: number[];
}

export type SymbolPrimitive =
  | {
      type: 'Path';
      commands: string;
      fill?: Color;
      stroke?: Stroke;
    }
  | {
      type: 'Circle';
      cx: number;
      cy: number;
      r: number;
      fill?: Color;
      stroke?: Stroke;
    }
  | {
      type: 'Text';
      content: string;
      offset_x: number;
      offset_y: number;
      font_size: number;
      color: Color;
    };

export interface DeclarativeSymbolDto {
  bbox: [number, number, number, number];
  anchor: [number, number];
  primitives: SymbolPrimitive[];
}

export interface DeclarativeLibraryDto {
  library_name: string;
  symbols: Record<string, DeclarativeSymbolDto>;
}

export interface SymbolConfig {
  id: string;
  svg_path: string;
  bbox: [number, number, number, number];
  anchor: [number, number];
}

export interface CompilerConfig {
  library_name: string;
  symbols: SymbolConfig[];
}

const COLOR_NAMES: Record<string, Color> = {
  transparent: { r: 0, g: 0, b: 0, a: 0 },
  none: { r: 0, g: 0, b: 0, a: 0 },
  black: { r: 0, g: 0, b: 0, a: 255 },
  white: { r: 255, g: 255, b: 255, a: 255 },
  red: { r: 255, g: 0, b: 0, a: 255 },
  green: { r: 0, g: 255, b: 0, a: 255 },
  blue: { r: 0, g: 0, b: 255, a: 255 },
  yellow: { r: 255, g: 255, b: 0, a: 255 },
  cyan: { r: 0, g: 255, b: 255, a: 255 },
  magenta: { r: 255, g: 0, b: 255, a: 255 },
  gray: { r: 128, g: 128, b: 128, a: 255 },
  grey: { r: 128, g: 128, b: 128, a: 255 },
  orange: { r: 255, g: 165, b: 0, a: 255 },
  purple: { r: 128, g: 0, b: 128, a: 255 },
  pink: { r: 255, g: 192, b: 203, a: 255 }
};

export function parseColor(colorStr: string | undefined, opacity = 1.0): Color | undefined {
  if (!colorStr) return undefined;
  colorStr = colorStr.trim().toLowerCase();
  if (colorStr === 'none' || colorStr === 'transparent') return undefined;

  let parsed: Color | undefined;
  if (colorStr.startsWith('#')) {
    const hex = colorStr.slice(1);
    if (hex.length === 3) {
      parsed = {
        r: parseInt(hex[0], 16) * 17,
        g: parseInt(hex[1], 16) * 17,
        b: parseInt(hex[2], 16) * 17,
        a: 255
      };
    } else if (hex.length === 4) {
      parsed = {
        r: parseInt(hex[0], 16) * 17,
        g: parseInt(hex[1], 16) * 17,
        b: parseInt(hex[2], 16) * 17,
        a: parseInt(hex[3], 16) * 17
      };
    } else if (hex.length === 6) {
      parsed = {
        r: parseInt(hex.slice(0, 2), 16),
        g: parseInt(hex.slice(2, 4), 16),
        b: parseInt(hex.slice(4, 6), 16),
        a: 255
      };
    } else if (hex.length === 8) {
      parsed = {
        r: parseInt(hex.slice(0, 2), 16),
        g: parseInt(hex.slice(2, 4), 16),
        b: parseInt(hex.slice(4, 6), 16),
        a: parseInt(hex.slice(6, 8), 16)
      };
    }
  } else {
    const rgbMatch = colorStr.match(/^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*(?:,\s*([\d.]+)\s*)?\)$/);
    if (rgbMatch) {
      const r = parseInt(rgbMatch[1], 10);
      const g = parseInt(rgbMatch[2], 10);
      const b = parseInt(rgbMatch[3], 10);
      const a = rgbMatch[4] !== undefined ? Math.round(parseFloat(rgbMatch[4]) * 255) : 255;
      parsed = { r, g, b, a };
    } else if (COLOR_NAMES[colorStr]) {
      parsed = { ...COLOR_NAMES[colorStr] };
    }
  }

  if (parsed) {
    parsed.a = Math.round(parsed.a * opacity);
    return parsed;
  }
  return undefined;
}

interface InheritedStyle {
  fill?: string;
  stroke?: string;
  strokeWidth?: string;
  strokeDasharray?: string;
  opacity: number;
}

export function compileSvg(svgContent: string): SymbolPrimitive[] {
  const parser = new XMLParser({
    ignoreAttributes: false,
    attributeNamePrefix: '',
    parseAttributeValue: false
  });

  const parsed = parser.parse(svgContent);
  const primitives: SymbolPrimitive[] = [];

  const defaultStyle: InheritedStyle = {
    opacity: 1.0
  };

  walk(parsed, defaultStyle, primitives);
  return primitives;
}

function parseFloatSafe(val: any, fallback = 0.0): number {
  if (val === undefined || val === null) return fallback;
  const num = parseFloat(val);
  return isNaN(num) ? fallback : num;
}

function parseDasharray(dashStr: string | undefined): number[] | undefined {
  if (!dashStr || dashStr === 'none') return undefined;
  const parts = dashStr.split(/[\s,]+/).map(parseFloat).filter(n => !isNaN(n));
  return parts.length > 0 ? parts : undefined;
}

function walk(node: any, inherited: InheritedStyle, primitives: SymbolPrimitive[]) {
  if (!node) return;

  if (Array.isArray(node)) {
    for (const child of node) {
      walk(child, inherited, primitives);
    }
    return;
  }

  if (typeof node === 'object') {
    // Collect attributes if this object represents a tag element
    // In fast-xml-parser, attributes are directly properties of the tag object
    const fill = node.fill !== undefined ? node.fill : inherited.fill;
    const stroke = node.stroke !== undefined ? node.stroke : inherited.stroke;
    const strokeWidth = node['stroke-width'] !== undefined ? node['stroke-width'] : inherited.strokeWidth;
    const strokeDasharray = node['stroke-dasharray'] !== undefined ? node['stroke-dasharray'] : inherited.strokeDasharray;
    const localOpacity = parseFloatSafe(node.opacity, 1.0);
    const opacity = inherited.opacity * localOpacity;

    const currentStyle: InheritedStyle = {
      fill,
      stroke,
      strokeWidth,
      strokeDasharray,
      opacity
    };

    // Check for specific tags
    for (const key of Object.keys(node)) {
      if (key === 'path') {
        const val = node[key];
        const paths = Array.isArray(val) ? val : [val];
        for (const p of paths) {
          if (!p.d) continue;
          const pFill = p.fill !== undefined ? p.fill : currentStyle.fill;
          const pStroke = p.stroke !== undefined ? p.stroke : currentStyle.stroke;
          const pStrokeWidth = p['stroke-width'] !== undefined ? p['stroke-width'] : currentStyle.strokeWidth;
          const pDash = p['stroke-dasharray'] !== undefined ? p['stroke-dasharray'] : currentStyle.strokeDasharray;
          const pOpacity = currentStyle.opacity * parseFloatSafe(p.opacity, 1.0);
          const pFillOpacity = pOpacity * parseFloatSafe(p['fill-opacity'], 1.0);
          const pStrokeOpacity = pOpacity * parseFloatSafe(p['stroke-opacity'], 1.0);

          const fillVal = parseColor(pFill, pFillOpacity);
          const strokeColorVal = parseColor(pStroke, pStrokeOpacity);
          const strokeWidthVal = parseFloatSafe(pStrokeWidth, 1.0);
          const strokeDasharrayVal = parseDasharray(pDash);

          const strokeVal: Stroke | undefined = strokeColorVal
            ? {
                color: strokeColorVal,
                width: strokeWidthVal,
                ...(strokeDasharrayVal ? { dash_array: strokeDasharrayVal } : {})
              }
            : undefined;

          primitives.push({
            type: 'Path',
            commands: p.d,
            ...(fillVal ? { fill: fillVal } : {}),
            ...(strokeVal ? { stroke: strokeVal } : {})
          });
        }
      } else if (key === 'circle') {
        const val = node[key];
        const circles = Array.isArray(val) ? val : [val];
        for (const c of circles) {
          const r = parseFloatSafe(c.r, 0.0);
          if (r <= 0) continue;
          const cx = parseFloatSafe(c.cx, 0.0);
          const cy = parseFloatSafe(c.cy, 0.0);

          const cFill = c.fill !== undefined ? c.fill : currentStyle.fill;
          const cStroke = c.stroke !== undefined ? c.stroke : currentStyle.stroke;
          const cStrokeWidth = c['stroke-width'] !== undefined ? c['stroke-width'] : currentStyle.strokeWidth;
          const cDash = c['stroke-dasharray'] !== undefined ? c['stroke-dasharray'] : currentStyle.strokeDasharray;
          const cOpacity = currentStyle.opacity * parseFloatSafe(c.opacity, 1.0);
          const cFillOpacity = cOpacity * parseFloatSafe(c['fill-opacity'], 1.0);
          const cStrokeOpacity = cOpacity * parseFloatSafe(c['stroke-opacity'], 1.0);

          const fillVal = parseColor(cFill, cFillOpacity);
          const strokeColorVal = parseColor(cStroke, cStrokeOpacity);
          const strokeWidthVal = parseFloatSafe(cStrokeWidth, 1.0);
          const strokeDasharrayVal = parseDasharray(cDash);

          const strokeVal: Stroke | undefined = strokeColorVal
            ? {
                color: strokeColorVal,
                width: strokeWidthVal,
                ...(strokeDasharrayVal ? { dash_array: strokeDasharrayVal } : {})
              }
            : undefined;

          primitives.push({
            type: 'Circle',
            cx,
            cy,
            r,
            ...(fillVal ? { fill: fillVal } : {}),
            ...(strokeVal ? { stroke: strokeVal } : {})
          });
        }
      } else if (key === 'text') {
        const val = node[key];
        const texts = Array.isArray(val) ? val : [val];
        for (const t of texts) {
          const content = t['#text'] !== undefined ? String(t['#text']).trim() : '';
          if (!content) continue;
          const x = parseFloatSafe(t.x, 0.0);
          const y = parseFloatSafe(t.y, 0.0);
          const fontSize = parseFloatSafe(t['font-size'], 12.0);
          
          const tFill = t.fill !== undefined ? t.fill : currentStyle.fill;
          const tOpacity = currentStyle.opacity * parseFloatSafe(t.opacity, 1.0);
          const tFillOpacity = tOpacity * parseFloatSafe(t['fill-opacity'], 1.0);
          const colorVal = parseColor(tFill, tFillOpacity) || { r: 0, g: 0, b: 0, a: 255 };

          primitives.push({
            type: 'Text',
            content,
            offset_x: x,
            offset_y: y,
            font_size: fontSize,
            color: colorVal
          });
        }
      } else if (key === 'g' || key === 'svg') {
        // Recurse into nested structures
        walk(node[key], currentStyle, primitives);
      } else if (typeof node[key] === 'object') {
        // Walk other tag elements like <defs>, <marker> if any (though usually skipped or treated as group)
        walk(node[key], currentStyle, primitives);
      }
    }
  }
}

export function compileLibrary(configPath: string, rootDir: string): DeclarativeLibraryDto {
  const absoluteConfigPath = path.resolve(configPath);
  const configContent = fs.readFileSync(absoluteConfigPath, 'utf-8');
  const config = JSON.parse(configContent) as CompilerConfig;

  const library: DeclarativeLibraryDto = {
    library_name: config.library_name,
    symbols: {}
  };

  const configDir = path.dirname(absoluteConfigPath);

  for (const symConfig of config.symbols) {
    const svgFullPath = path.isAbsolute(symConfig.svg_path)
      ? symConfig.svg_path
      : path.resolve(configDir, symConfig.svg_path);

    if (!fs.existsSync(svgFullPath)) {
      throw new Error(`SVG file not found: ${svgFullPath} for symbol ID ${symConfig.id}`);
    }

    const svgContent = fs.readFileSync(svgFullPath, 'utf-8');
    const primitives = compileSvg(svgContent);

    library.symbols[symConfig.id] = {
      bbox: symConfig.bbox,
      anchor: symConfig.anchor,
      primitives
    };
  }

  return library;
}
