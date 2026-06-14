import { compileSvg, parseColor } from './compiler.js';
import * as assert from 'assert';

console.log('Running color parser tests...');
assert.deepStrictEqual(parseColor('#ff0000'), { r: 255, g: 0, b: 0, a: 255 });
assert.deepStrictEqual(parseColor('#f0a8'), { r: 255, g: 0, b: 170, a: 136 });
assert.deepStrictEqual(parseColor('red'), { r: 255, g: 0, b: 0, a: 255 });
assert.deepStrictEqual(parseColor('rgb(10, 20, 30)'), { r: 10, g: 20, b: 30, a: 255 });
assert.deepStrictEqual(parseColor('rgba(10, 20, 30, 0.5)'), { r: 10, g: 20, b: 30, a: 128 });
assert.deepStrictEqual(parseColor('none'), undefined);
console.log('Color parser tests passed!');

console.log('Running SVG compilation tests...');
const svg = `
<svg width="24" height="24">
  <g fill="none" stroke="red" stroke-width="2">
    <path d="M 0,0 L 10,10" />
    <circle cx="12" cy="12" r="5" fill="blue" opacity="0.5" />
  </g>
</svg>
`;

const primitives = compileSvg(svg);
assert.strictEqual(primitives.length, 2);

assert.deepStrictEqual(primitives[0], {
  type: 'Path',
  commands: 'M 0,0 L 10,10',
  stroke: {
    color: { r: 255, g: 0, b: 0, a: 255 },
    width: 2
  }
});

assert.deepStrictEqual(primitives[1], {
  type: 'Circle',
  cx: 12,
  cy: 12,
  r: 5,
  fill: { r: 0, g: 0, b: 255, a: 128 },
  stroke: {
    color: { r: 255, g: 0, b: 0, a: 128 },
    width: 2
  }
});

console.log('SVG compilation tests passed!');
