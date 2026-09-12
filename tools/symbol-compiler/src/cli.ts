import { Command } from 'commander';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { compileLibrary, CompilerError } from './compiler.js';

const program = new Command();

program
  .name('olayer-symbol-compiler')
  .description('CLI to compile SVGs into Olayer declarative JSON libraries.')
  .version('0.1.0')
  .requiredOption('-c, --config <path>', 'Path to the symbols.config.json configuration file')
  .requiredOption('-o, --output <path>', 'Path to write the compiled JSON library output')
  .option('--strict', 'Fail when an unsupported SVG element is encountered')
  .option('--verbose', 'Print compiler warnings')
  .action((options) => {
    let temporaryPath: string | undefined;
    try {
      const configPath = path.resolve(options.config);
      const outputPath = path.resolve(options.output);

      console.log(`Starting compilation of symbols using config: ${configPath}...`);

      const compiled = compileLibrary(configPath, process.cwd(), {
        strict: Boolean(options.strict),
        onWarning: options.verbose ? (message) => console.warn(`Warning: ${message}`) : undefined
      });

      const outputDir = path.dirname(outputPath);
      fs.mkdirSync(outputDir, { recursive: true });

      temporaryPath = path.join(outputDir, `.${path.basename(outputPath)}.${process.pid}.tmp`);
      fs.writeFileSync(temporaryPath, `${JSON.stringify(compiled, null, 2)}\n`, { encoding: 'utf-8', flag: 'wx' });
      fs.renameSync(temporaryPath, outputPath);
      temporaryPath = undefined;
      console.log(`Successfully compiled symbols library "${compiled.library_name}" to: ${outputPath}`);
    } catch (error: unknown) {
      if (temporaryPath) {
        try { fs.unlinkSync(temporaryPath); } catch { /* Best effort cleanup. */ }
      }
      const message = error instanceof Error ? error.message : String(error);
      const code = error instanceof CompilerError ? error.code : 'UNKNOWN';
      console.error(`Compilation failed [${code}]: ${message}`);
      process.exitCode = 1;
    }
  });

program.parse(process.argv);
