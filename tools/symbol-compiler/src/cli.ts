import { Command } from 'commander';
import * as fs from 'fs';
import * as path from 'path';
import { compileLibrary } from './compiler.js';

const program = new Command();

program
  .name('olayer-symbol-compiler')
  .description('CLI to compile SVGs into Olayer declarative JSON libraries.')
  .version('0.1.0')
  .requiredOption('-c, --config <path>', 'Path to the symbols.config.json configuration file')
  .requiredOption('-o, --output <path>', 'Path to write the compiled JSON library output')
  .action((options) => {
    try {
      const configPath = path.resolve(options.config);
      const outputPath = path.resolve(options.output);

      console.log(`Starting compilation of symbols using config: ${configPath}...`);
      
      const compiled = compileLibrary(configPath, process.cwd());
      
      // Ensure directory exists
      const outputDir = path.dirname(outputPath);
      if (!fs.existsSync(outputDir)) {
        fs.mkdirSync(outputDir, { recursive: true });
      }

      fs.writeFileSync(outputPath, JSON.stringify(compiled, null, 2), 'utf-8');
      console.log(`Successfully compiled symbols library "${compiled.library_name}" to: ${outputPath}`);
    } catch (error: any) {
      console.error('Compilation failed:', error.message);
      process.exit(1);
    }
  });

program.parse(process.argv);
