import * as assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { compileLibrary, compileSvg, CompilerError, normalizePath, parseColor } from './compiler.js';

function expectCompilerError(action: () => unknown, code: CompilerError['code']): void {
  assert.throws(action, (error: unknown) => error instanceof CompilerError && error.code === code);
}

assert.deepEqual(parseColor('#ff0000'), { r: 255, g: 0, b: 0, a: 255 });
assert.deepEqual(parseColor('#f0a8'), { r: 255, g: 0, b: 170, a: 136 });
assert.deepEqual(parseColor('red'), { r: 255, g: 0, b: 0, a: 255 });
assert.deepEqual(parseColor('rgb(10, 20, 30)'), { r: 10, g: 20, b: 30, a: 255 });
assert.deepEqual(parseColor('rgba(10, 20, 30, 0.5)'), { r: 10, g: 20, b: 30, a: 128 });
assert.deepEqual(parseColor('rgb(100% 0% 50%)'), { r: 255, g: 0, b: 128, a: 255 });
assert.equal(parseColor('#gg0000'), undefined);
assert.equal(parseColor('rgb(300, 0, 0)'), undefined);
assert.equal(parseColor('none'), undefined);

assert.equal(normalizePath('m 1 2 h 3 v 4 l -2 -1 z'), 'M 1 2 L 4 2 L 4 6 L 2 5 Z');
assert.equal(normalizePath('M0 0 L10 0', [1, 0, 0, 1, 5, 6]), 'M 5 6 L 15 6');
expectCompilerError(() => normalizePath('M 0 0 C 1 2 3 4 5 6'), 'UNSUPPORTED');

const svg = `
<svg width="24" height="24">
  <defs><path d="M 99 99 L 100 100" /></defs>
  <g fill="none" stroke="red" stroke-width="2" opacity="0.5" transform="translate(1, 2)">
    <path d="M 0,0 L 10,10" />
    <circle cx="12" cy="12" r="5" fill="blue" />
  </g>
  <text x="4" y="5" fill="green">Label</text>
</svg>
`;
const primitives = compileSvg(svg);
assert.equal(primitives.length, 3);
assert.deepEqual(primitives[0], {
  type: 'Path',
  commands: 'M 1 2 L 11 12',
  stroke: { color: { r: 255, g: 0, b: 0, a: 128 }, width: 2 }
});
assert.deepEqual(primitives[1], {
  type: 'Circle',
  cx: 13,
  cy: 14,
  r: 5,
  fill: { r: 0, g: 0, b: 255, a: 128 },
  stroke: { color: { r: 255, g: 0, b: 0, a: 128 }, width: 2 }
});
assert.deepEqual(primitives[2], {
  type: 'Text',
  content: 'Label',
  offset_x: 4,
  offset_y: 5,
  font_size: 12,
  color: { r: 0, g: 128, b: 0, a: 255 }
});

assert.deepEqual(compileSvg('<svg><g style="fill: blue; fill-opacity: 0.25"><circle cx="1" cy="2" r="3" /></g></svg>')[0], {
  type: 'Circle', cx: 1, cy: 2, r: 3, fill: { r: 0, g: 0, b: 255, a: 64 }
});
expectCompilerError(() => compileSvg('<svg><rect width="1" height="1" /></svg>', { strict: true }), 'UNSUPPORTED');

const temporaryDirectory = fs.mkdtempSync(path.join(os.tmpdir(), 'olayer-symbol-compiler-'));
try {
  fs.writeFileSync(path.join(temporaryDirectory, 'symbol.svg'), '<svg><circle cx="1" cy="2" r="3" /></svg>');
  fs.writeFileSync(path.join(temporaryDirectory, 'config.json'), JSON.stringify({
    library_name: 'test',
    symbols: [{ id: 'test:circle', svg_path: 'symbol.svg', bbox: [0, 0, 4, 5], anchor: [1, 2] }]
  }));
  const library = compileLibrary(path.join(temporaryDirectory, 'config.json'));
  assert.equal(library.symbols['test:circle'].primitives.length, 1);

  fs.writeFileSync(path.join(temporaryDirectory, 'duplicate.json'), JSON.stringify({
    library_name: 'test',
    symbols: [
      { id: 'same', svg_path: 'symbol.svg', bbox: [0, 0, 4, 5], anchor: [1, 2] },
      { id: 'same', svg_path: 'symbol.svg', bbox: [0, 0, 4, 5], anchor: [1, 2] }
    ]
  }));
  expectCompilerError(() => compileLibrary(path.join(temporaryDirectory, 'duplicate.json')), 'CONFIG');
} finally {
  fs.rmSync(temporaryDirectory, { recursive: true, force: true });
}

console.log('Symbol compiler tests passed.');
