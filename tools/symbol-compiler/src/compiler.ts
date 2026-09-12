import * as fs from 'node:fs';
import * as path from 'node:path';
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
  | { type: 'Path'; commands: string; fill?: Color; stroke?: Stroke }
  | { type: 'Circle'; cx: number; cy: number; r: number; fill?: Color; stroke?: Stroke }
  | { type: 'Text'; content: string; offset_x: number; offset_y: number; font_size: number; color: Color };

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

export interface CompileOptions {
  strict?: boolean;
  onWarning?: (message: string) => void;
}

export class CompilerError extends Error {
  public readonly code: 'CONFIG' | 'SVG' | 'UNSUPPORTED' | 'IO';

  public constructor(code: CompilerError['code'], message: string) {
    super(message);
    this.name = 'CompilerError';
    this.code = code;
  }
}

const COLOR_NAMES: Record<string, Color> = {
  transparent: { r: 0, g: 0, b: 0, a: 0 },
  none: { r: 0, g: 0, b: 0, a: 0 },
  black: { r: 0, g: 0, b: 0, a: 255 },
  white: { r: 255, g: 255, b: 255, a: 255 },
  red: { r: 255, g: 0, b: 0, a: 255 },
  green: { r: 0, g: 128, b: 0, a: 255 },
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

const SUPPORTED_TAGS = new Set(['svg', 'g', 'path', 'circle', 'text']);
const IGNORED_TAGS = new Set(['defs', 'metadata', 'title', 'desc', 'marker', 'clipPath', 'mask', 'style', 'symbol']);
const NUMBER_PATTERN = '[+-]?(?:\\d+\\.?\\d*|\\.\\d+)(?:[eE][+-]?\\d+)?';

export function parseColor(colorStr: string | undefined, opacity = 1): Color | undefined {
  if (!colorStr) return undefined;
  const value = colorStr.trim().toLowerCase();
  if (value === 'none' || value === 'transparent') return undefined;

  let parsed: Color | undefined;
  if (value.startsWith('#')) {
    const hex = value.slice(1);
    if (![3, 4, 6, 8].includes(hex.length) || !/^[0-9a-f]+$/i.test(hex)) return undefined;
    const expand = (part: string) => part.length === 1 ? parseInt(part + part, 16) : parseInt(part, 16);
    parsed = hex.length <= 4
      ? { r: expand(hex[0]), g: expand(hex[1]), b: expand(hex[2]), a: hex.length === 4 ? expand(hex[3]) : 255 }
      : { r: expand(hex.slice(0, 2)), g: expand(hex.slice(2, 4)), b: expand(hex.slice(4, 6)), a: hex.length === 8 ? expand(hex.slice(6, 8)) : 255 };
  } else {
    const rgb = value.match(new RegExp(`^rgba?\\(\\s*(${NUMBER_PATTERN})%?\\s*[, ]\\s*(${NUMBER_PATTERN})%?\\s*[, ]\\s*(${NUMBER_PATTERN})%?(?:\\s*[,/]\\s*(${NUMBER_PATTERN})%?)?\\s*\\)$`));
    if (rgb) {
      const channels = [rgb[1], rgb[2], rgb[3]].map(Number);
      const isPercent = value.includes('%');
      if (!channels.every(Number.isFinite)) return undefined;
      const alpha = rgb[4] === undefined ? 255 : (value.includes('%') ? Number(rgb[4]) * 255 / 100 : Number(rgb[4]) * 255);
      if (!Number.isFinite(alpha) || channels.some(channel => channel < 0 || channel > (isPercent ? 100 : 255))) return undefined;
      parsed = {
        r: isPercent ? Math.round(channels[0] * 255 / 100) : Math.round(channels[0]),
        g: isPercent ? Math.round(channels[1] * 255 / 100) : Math.round(channels[1]),
        b: isPercent ? Math.round(channels[2] * 255 / 100) : Math.round(channels[2]),
        a: Math.round(alpha)
      };
    } else if (COLOR_NAMES[value]) {
      parsed = { ...COLOR_NAMES[value] };
    }
  }

  if (!parsed || !Number.isFinite(opacity) || opacity < 0 || opacity > 1) return undefined;
  parsed.a = Math.max(0, Math.min(255, Math.round(parsed.a * opacity)));
  return parsed;
}

interface Style {
  fill?: string;
  stroke?: string;
  strokeWidth?: string;
  strokeDasharray?: string;
  fillOpacity: number;
  strokeOpacity: number;
  opacity: number;
  display: string;
  visibility: string;
}

interface Point { x: number; y: number }
type Matrix = [number, number, number, number, number, number];

const IDENTITY: Matrix = [1, 0, 0, 1, 0, 0];

function parseNumber(value: unknown, field: string, fallback?: number): number {
  if (value === undefined || value === null || value === '') {
    if (fallback !== undefined) return fallback;
    throw new CompilerError('SVG', `Missing numeric value for ${field}`);
  }
  const result = Number.parseFloat(String(value));
  if (!Number.isFinite(result)) throw new CompilerError('SVG', `Invalid numeric value for ${field}: ${String(value)}`);
  return result;
}

function parseOpacity(value: unknown, field: string, fallback = 1): number {
  const result = parseNumber(value, field, fallback);
  if (result < 0 || result > 1) throw new CompilerError('SVG', `${field} must be between 0 and 1`);
  return result;
}

function parseDasharray(value: string | undefined): number[] | undefined {
  if (!value || value.trim().toLowerCase() === 'none') return undefined;
  const result = value.split(/[\s,]+/).filter(Boolean).map((part) => parseNumber(part, 'stroke-dasharray'));
  if (result.some(number => number < 0)) throw new CompilerError('SVG', 'stroke-dasharray values cannot be negative');
  return result.length > 0 ? result : undefined;
}

function parseStyleAttribute(value: unknown): Record<string, string> {
  if (typeof value !== 'string') return {};
  return Object.fromEntries(value.split(';').map(part => part.trim()).filter(Boolean).map(part => {
    const separator = part.indexOf(':');
    return separator === -1 ? ['', ''] : [part.slice(0, separator).trim(), part.slice(separator + 1).trim()];
  }).filter(([key]) => key));
}

function styleFor(node: Record<string, unknown>, inherited: Style): Style {
  const inline = parseStyleAttribute(node.style);
  const value = (name: string, inheritedValue?: string) => inline[name] ?? (node[name] !== undefined ? String(node[name]) : inheritedValue);
  return {
    fill: value('fill', inherited.fill),
    stroke: value('stroke', inherited.stroke),
    strokeWidth: value('stroke-width', inherited.strokeWidth),
    strokeDasharray: value('stroke-dasharray', inherited.strokeDasharray),
    fillOpacity: parseOpacity(value('fill-opacity'), 'fill-opacity', inherited.fillOpacity),
    strokeOpacity: parseOpacity(value('stroke-opacity'), 'stroke-opacity', inherited.strokeOpacity),
    opacity: inherited.opacity * parseOpacity(value('opacity'), 'opacity'),
    display: value('display', inherited.display) ?? 'inline',
    visibility: value('visibility', inherited.visibility) ?? 'visible'
  };
}

function multiply(left: Matrix, right: Matrix): Matrix {
  return [
    left[0] * right[0] + left[2] * right[1],
    left[1] * right[0] + left[3] * right[1],
    left[0] * right[2] + left[2] * right[3],
    left[1] * right[2] + left[3] * right[3],
    left[0] * right[4] + left[2] * right[5] + left[4],
    left[1] * right[4] + left[3] * right[5] + left[5]
  ];
}

function transformPoint(matrix: Matrix, point: Point): Point {
  return { x: matrix[0] * point.x + matrix[2] * point.y + matrix[4], y: matrix[1] * point.x + matrix[3] * point.y + matrix[5] };
}

function parseTransform(value: unknown): Matrix {
  if (!value) return IDENTITY;
  let result = IDENTITY;
  const expression = String(value);
  const matcher = /(matrix|translate|scale|rotate)\s*\(([^)]*)\)/g;
  let match: RegExpExecArray | null;
  let consumed = '';
  while ((match = matcher.exec(expression)) !== null) {
    consumed += match[0];
    const values = match[2].split(/[\s,]+/).filter(Boolean).map((part, index) => parseNumber(part, `transform ${match?.[1]}[${index}]`));
    let matrix: Matrix;
    switch (match[1]) {
      case 'matrix':
        if (values.length !== 6) throw new CompilerError('SVG', 'matrix transform requires six values');
        matrix = values as Matrix;
        break;
      case 'translate':
        if (values.length < 1 || values.length > 2) throw new CompilerError('SVG', 'translate transform requires one or two values');
        matrix = [1, 0, 0, 1, values[0], values[1] ?? 0];
        break;
      case 'scale':
        if (values.length < 1 || values.length > 2) throw new CompilerError('SVG', 'scale transform requires one or two values');
        matrix = [values[0], 0, 0, values[1] ?? values[0], 0, 0];
        break;
      default: {
        if (values.length < 1 || values.length > 3) throw new CompilerError('SVG', 'rotate transform requires one or three values');
        const angle = values[0] * Math.PI / 180;
        const rotation: Matrix = [Math.cos(angle), Math.sin(angle), -Math.sin(angle), Math.cos(angle), 0, 0];
        if (values.length === 1) matrix = rotation;
        else {
          const center = [values[1], values[2]] as [number, number];
          matrix = multiply(multiply([1, 0, 0, 1, center[0], center[1]], rotation), [1, 0, 0, 1, -center[0], -center[1]]);
        }
      }
    }
    result = multiply(result, matrix);
  }
  if (consumed.replace(/[\s,]+/g, '') !== expression.replace(/[\s,]+/g, '')) throw new CompilerError('UNSUPPORTED', `Unsupported transform: ${expression}`);
  return result;
}

interface PathState { current: Point; start: Point; command: string; }

function tokenizePath(value: string): Array<string | number> {
  const tokens: Array<string | number> = [];
  const matcher = new RegExp(`[a-zA-Z]|${NUMBER_PATTERN}`, 'g');
  const matches = value.match(matcher) ?? [];
  const compact = value.replace(/[\s,]+/g, '');
  if (matches.join('') !== compact) throw new CompilerError('UNSUPPORTED', `Invalid path data: ${value}`);
  for (const token of matches) tokens.push(/[a-zA-Z]/.test(token) ? token : Number(token));
  return tokens;
}

function formatNumber(value: number): string {
  const normalized = Math.abs(value) < 1e-9 ? 0 : value;
  return Number(normalized.toFixed(6)).toString();
}

export function normalizePath(value: string, transform: Matrix = IDENTITY): string {
  const tokens = tokenizePath(value);
  const output: string[] = [];
  const state: PathState = { current: { x: 0, y: 0 }, start: { x: 0, y: 0 }, command: '' };
  let index = 0;
  const commandArguments: Record<string, number> = { M: 2, L: 2, H: 1, V: 1, Z: 0 };
  const point = (x: number, y: number) => transformPoint(transform, { x, y });
  while (index < tokens.length) {
    if (typeof tokens[index] === 'string') state.command = tokens[index++] as string;
    const upper = state.command.toUpperCase();
    if (!commandArguments[upper]) {
      if (upper !== 'Z') throw new CompilerError('UNSUPPORTED', `Unsupported path command: ${state.command}`);
      output.push('Z');
      state.current = state.start;
      state.command = '';
      continue;
    }
    const argumentCount = commandArguments[upper];
    if (index + argumentCount > tokens.length || tokens.slice(index, index + argumentCount).some(token => typeof token === 'string')) {
      throw new CompilerError('SVG', `Incomplete path command: ${state.command}`);
    }
    const args = tokens.slice(index, index + argumentCount) as number[];
    index += argumentCount;
    const relative = state.command !== upper;
    let next: Point;
    if (upper === 'M' || upper === 'L') {
      next = { x: relative ? state.current.x + args[0] : args[0], y: relative ? state.current.y + args[1] : args[1] };
      const transformed = point(next.x, next.y);
      output.push(`${upper === 'M' ? 'M' : 'L'} ${formatNumber(transformed.x)} ${formatNumber(transformed.y)}`);
      state.current = next;
      if (upper === 'M') { state.start = next; state.command = relative ? 'l' : 'L'; }
    } else if (upper === 'H') {
      next = { x: relative ? state.current.x + args[0] : args[0], y: state.current.y };
      const transformed = point(next.x, next.y);
      output.push(`L ${formatNumber(transformed.x)} ${formatNumber(transformed.y)}`);
      state.current = next;
    } else {
      next = { x: state.current.x, y: relative ? state.current.y + args[0] : args[0] };
      const transformed = point(next.x, next.y);
      output.push(`L ${formatNumber(transformed.x)} ${formatNumber(transformed.y)}`);
      state.current = next;
    }
  }
  return output.join(' ');
}

function warning(options: CompileOptions, message: string): void {
  if (options.strict) throw new CompilerError('UNSUPPORTED', message);
  options.onWarning?.(message);
}

function primitiveStyle(style: Style, element: Record<string, unknown>): { fill?: Color; stroke?: Stroke } {
  const opacity = style.opacity;
  const fill = parseColor(style.fill, opacity * style.fillOpacity);
  const strokeColor = parseColor(style.stroke, opacity * style.strokeOpacity);
  const strokeWidth = parseNumber(style.strokeWidth, 'stroke-width', 1);
  const dash = parseDasharray(style.strokeDasharray);
  const stroke = strokeColor ? { color: strokeColor, width: strokeWidth, ...(dash ? { dash_array: dash } : {}) } : undefined;
  return { ...(fill ? { fill } : {}), ...(stroke ? { stroke } : {}) };
}

function walk(node: unknown, inherited: Style, transform: Matrix, primitives: SymbolPrimitive[], options: CompileOptions, context: string): void {
  if (!node) return;
  if (Array.isArray(node)) {
    node.forEach((child, index) => walk(child, inherited, transform, primitives, options, `${context}[${index}]`));
    return;
  }
  if (typeof node !== 'object') return;
  const object = node as Record<string, any>;
  const currentStyle = styleFor(object, inherited);
  const currentTransform = multiply(transform, parseTransform(object.transform));
  if (currentStyle.display === 'none' || currentStyle.visibility === 'hidden') return;

  for (const [tag, value] of Object.entries(object)) {
    if (tag.startsWith('@') || tag === '#text' || tag === 'style' || tag === 'transform') continue;
    const elements = Array.isArray(value) ? value : [value];
    if (IGNORED_TAGS.has(tag)) continue;
    if (!SUPPORTED_TAGS.has(tag)) {
      if (typeof value === 'object') warning(options, `${context}: unsupported SVG element <${tag}>`);
      continue;
    }
    for (const element of elements) {
      if (tag === 'path') {
        const commands = normalizePath(String(element.d ?? ''), multiply(currentTransform, parseTransform(element.transform)));
        if (!commands) throw new CompilerError('SVG', `${context}/path has no path data`);
        primitives.push({ type: 'Path', commands, ...primitiveStyle(styleFor(element, currentStyle), element) });
      } else if (tag === 'circle') {
        const circleStyle = styleFor(element, currentStyle);
        const circleTransform = multiply(currentTransform, parseTransform(element.transform));
        const center = transformPoint(circleTransform, { x: parseNumber(element.cx, 'circle.cx', 0), y: parseNumber(element.cy, 'circle.cy', 0) });
        const radius = parseNumber(element.r, 'circle.r');
        const scaleX = Math.hypot(circleTransform[0], circleTransform[1]);
        const scaleY = Math.hypot(circleTransform[2], circleTransform[3]);
        if (Math.abs(scaleX - scaleY) > 1e-6) throw new CompilerError('UNSUPPORTED', `${context}/circle uses a non-uniform transform`);
        primitives.push({ type: 'Circle', cx: center.x, cy: center.y, r: radius * scaleX, ...primitiveStyle(circleStyle, element) });
      } else if (tag === 'text') {
        const textStyle = styleFor(element, currentStyle);
        const content = typeof element === 'string' ? element.trim() : String(element['#text'] ?? '').trim();
        if (!content) continue;
        const position = transformPoint(currentTransform, { x: parseNumber(element.x, 'text.x', 0), y: parseNumber(element.y, 'text.y', 0) });
        const color = parseColor(textStyle.fill, textStyle.opacity * textStyle.fillOpacity);
        if (!color) continue;
        primitives.push({ type: 'Text', content, offset_x: position.x, offset_y: position.y, font_size: parseNumber(element['font-size'], 'text.font-size', 12), color });
      } else {
        walk(element, currentStyle, currentTransform, primitives, options, `${context}/${tag}`);
      }
    }
  }
}

function validateConfig(value: unknown, configPath: string): CompilerConfig {
  if (!value || typeof value !== 'object') throw new CompilerError('CONFIG', `${configPath}: configuration must be an object`);
  const config = value as Record<string, unknown>;
  if (typeof config.library_name !== 'string' || !config.library_name.trim()) throw new CompilerError('CONFIG', `${configPath}: library_name must be a non-empty string`);
  if (!Array.isArray(config.symbols) || config.symbols.length === 0) throw new CompilerError('CONFIG', `${configPath}: symbols must be a non-empty array`);
  const ids = new Set<string>();
  const symbols = config.symbols.map((raw, index) => {
    const context = `${configPath}: symbols[${index}]`;
    if (!raw || typeof raw !== 'object') throw new CompilerError('CONFIG', `${context} must be an object`);
    const symbol = raw as Record<string, unknown>;
    if (typeof symbol.id !== 'string' || !symbol.id.trim()) throw new CompilerError('CONFIG', `${context}.id must be a non-empty string`);
    if (ids.has(symbol.id)) throw new CompilerError('CONFIG', `${context}.id duplicates symbol "${symbol.id}"`);
    ids.add(symbol.id);
    if (typeof symbol.svg_path !== 'string' || !symbol.svg_path.trim()) throw new CompilerError('CONFIG', `${context}.svg_path must be a non-empty string`);
    const tuple = (field: 'bbox' | 'anchor', length: number): number[] => {
      if (!Array.isArray(symbol[field]) || symbol[field].length !== length || symbol[field].some(item => typeof item !== 'number' || !Number.isFinite(item))) throw new CompilerError('CONFIG', `${context}.${field} must contain ${length} finite numbers`);
      return symbol[field] as number[];
    };
    const bbox = tuple('bbox', 4) as [number, number, number, number];
    if (bbox[0] > bbox[2] || bbox[1] > bbox[3]) throw new CompilerError('CONFIG', `${context}.bbox must have min values before max values`);
    const anchor = tuple('anchor', 2) as [number, number];
    return { id: symbol.id, svg_path: symbol.svg_path, bbox, anchor };
  });
  return { library_name: config.library_name, symbols };
}

export function compileSvg(svgContent: string, options: CompileOptions = {}): SymbolPrimitive[] {
  if (!svgContent.trim()) throw new CompilerError('SVG', 'SVG content is empty');
  const parser = new XMLParser({ ignoreAttributes: false, attributeNamePrefix: '', parseAttributeValue: false, processEntities: false, allowBooleanAttributes: true });
  let parsed: unknown;
  try { parsed = parser.parse(svgContent); } catch (error) { throw new CompilerError('SVG', `Failed to parse SVG: ${error instanceof Error ? error.message : String(error)}`); }
  const primitives: SymbolPrimitive[] = [];
  walk(parsed, { fill: 'black', fillOpacity: 1, strokeOpacity: 1, opacity: 1, display: 'inline', visibility: 'visible' }, IDENTITY, primitives, options, 'svg');
  return primitives;
}

export function compileLibrary(configPath: string, _rootDir?: string, options: CompileOptions = {}): DeclarativeLibraryDto {
  const absoluteConfigPath = path.resolve(configPath);
  let configContent: string;
  try { configContent = fs.readFileSync(absoluteConfigPath, 'utf-8'); } catch (error) { throw new CompilerError('IO', `Cannot read configuration ${absoluteConfigPath}: ${error instanceof Error ? error.message : String(error)}`); }
  let rawConfig: unknown;
  try { rawConfig = JSON.parse(configContent); } catch (error) { throw new CompilerError('CONFIG', `Invalid JSON in ${absoluteConfigPath}: ${error instanceof Error ? error.message : String(error)}`); }
  const config = validateConfig(rawConfig, absoluteConfigPath);
  const library: DeclarativeLibraryDto = { library_name: config.library_name, symbols: {} };
  const configDir = path.dirname(absoluteConfigPath);
  for (const [index, symbol] of config.symbols.entries()) {
    const svgPath = path.isAbsolute(symbol.svg_path) ? symbol.svg_path : path.resolve(configDir, symbol.svg_path);
    let svgContent: string;
    try { svgContent = fs.readFileSync(svgPath, 'utf-8'); } catch (error) { throw new CompilerError('IO', `${absoluteConfigPath}: symbols[${index}] (${symbol.id}): cannot read SVG ${svgPath}: ${error instanceof Error ? error.message : String(error)}`); }
    let primitives: SymbolPrimitive[];
    try { primitives = compileSvg(svgContent, options); } catch (error) {
      if (error instanceof CompilerError) throw new CompilerError(error.code, `${absoluteConfigPath}: symbols[${index}] (${symbol.id}): ${error.message}`);
      throw error;
    }
    if (primitives.length === 0) throw new CompilerError('SVG', `${absoluteConfigPath}: symbols[${index}] (${symbol.id}): SVG has no supported renderable primitives`);
    library.symbols[symbol.id] = { bbox: symbol.bbox, anchor: symbol.anchor, primitives };
  }
  return library;
}
