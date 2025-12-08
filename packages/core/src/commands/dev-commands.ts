// src/commands/dev-commands.ts - Ultra-fast development commands like tsx
import { Command } from 'commander';
import { DevManager } from '../dev-manager';
import { loggerManager } from '../logger-manager';
import chalk from 'chalk';
import figures from 'figures';

export function addDevCommands(program: Command): { cleanupDev: () => void } {
  let devManager: DevManager | null = null;

  // Ultra-fast dev command optimized for speed
  program
    .command('dev [file]')
    .description('Start ultra-fast TypeScript development server (like tsx)')
    .option(
      '-w, --watch <patterns>',
      'Watch patterns (comma-separated)',
      'src/**/*'
    )
    .option(
      '-i, --ignore <patterns>',
      'Ignore patterns (comma-separated)',
      'node_modules,dist,build,.git'
    )
    .option(
      '-e, --ext <extensions>',
      'File extensions to watch',
      'ts,tsx,js,jsx'
    )
    .option('-d, --delay <ms>', 'Restart delay in milliseconds', parseInt, 100)
    .option('--fast', 'Ultra-fast mode (50ms delay)')
    .option('--no-clear', "Don't clear console on restart")
    .option('--no-color', 'Disable colored output')
    .option('-q, --quiet', 'Minimal output')
    .option('-v, --verbose', 'Verbose logging')
    .option('--inspect', 'Enable Node.js inspector')
    .option('--inspect-brk', 'Enable Node.js inspector with breakpoint')
    .option('--env <file>', 'Environment file to load', '.env')
    .option('--exec <command>', 'Custom command to execute')
    .option('--tsconfig <path>', 'TypeScript config file path')
    .option('--no-source-maps', 'Disable source map generation')
    .option('--transpile-only', 'Skip type checking (faster)')
    .option('--node-args <args>', 'Node.js arguments (comma-separated)', '')
    .action(async (file, options) => {
      try {
        const targetFile = file || 'src/index.ts';
        const delay = options.fast ? 50 : options.delay;

        if (!options.quiet) {
          console.log(''); // Empty line for better visual separation
          loggerManager.printLine(
            `${chalk.green(figures.play)} Starting ${chalk.cyan('neex dev')} for ${chalk.cyan(targetFile)}`,
            'info'
          );
        }

        devManager = new DevManager({
          file: targetFile,
          watch: options.watch.split(',').map((p: string) => p.trim()),
          ignore: options.ignore.split(',').map((p: string) => p.trim()),
          extensions: options.ext.split(',').map((e: string) => e.trim()),
          delay: delay,
          color: options.color,
          quiet: options.quiet,
          verbose: options.verbose,
          clearConsole: options.clear,
          inspect: options.inspect,
          inspectBrk: options.inspectBrk,
          envFile: options.env,
          execCommand: options.exec,
          tsConfig: options.tsconfig,
          sourceMaps: options.sourceMaps,
          transpileOnly: options.transpileOnly,
          nodeArgs: options.nodeArgs
            ? options.nodeArgs.split(',').map((arg: string) => arg.trim())
            : [],
        });

        // --- Signal Handlers for Dev ---
        let isShuttingDown = false;
        const cleanupAndExit = () => {
          if (isShuttingDown) return;
          isShuttingDown = true;

          if (devManager) {
            loggerManager.printLine(
              `\n${chalk.yellow('⏹')} Received SIGINT, shutting down...`,
              'info'
            );
            devManager.stop().then(() => process.exit(0));
          }
        };

        const sigintHandler = () => cleanupAndExit();
        const sigtermHandler = () => cleanupAndExit();

        process.on('SIGINT', sigintHandler);
        process.on('SIGTERM', sigtermHandler);

        await devManager.start();
      } catch (error: unknown) {
        if (error instanceof Error) {
          loggerManager.printLine(
            `${chalk.red('✖')} ${error.message}`,
            'error'
          );
        } else {
          loggerManager.printLine(
            `${chalk.red('✖')} Unknown error occurred`,
            'error'
          );
        }
        process.exit(1);
      }
    });

  // Clean cache command
  program
    .command('dev:clean')
    .description('Clean development cache and temp files')
    .action(() => {
      const path = require('path');
      const fs = require('fs');

      const tempDir = path.join(process.cwd(), '.neex-temp');
      const nodeModulesCache = path.join(process.cwd(), 'node_modules/.cache');

      let cleaned = false;

      if (fs.existsSync(tempDir)) {
        fs.rmSync(tempDir, { recursive: true, force: true });
        cleaned = true;
      }

      if (fs.existsSync(nodeModulesCache)) {
        try {
          fs.rmSync(nodeModulesCache, { recursive: true, force: true });
          cleaned = true;
        } catch (error) {
          // Ignore cache cleanup errors
        }
      }

      if (cleaned) {
        loggerManager.printLine(
          `${chalk.green('✓')} Cache cleaned successfully`,
          'info'
        );
      } else {
        loggerManager.printLine(
          `${chalk.blue('ℹ')} No cache to clean`,
          'info'
        );
      }
    });

  // TypeScript config check
  program
    .command('dev:check')
    .description('Check TypeScript configuration')
    .option('--tsconfig <path>', 'TypeScript config file path')
    .action(options => {
      const path = require('path');
      const fs = require('fs');

      const configPath = options.tsconfig || 'tsconfig.json';

      if (!fs.existsSync(configPath)) {
        loggerManager.printLine(
          `${chalk.red('✖')} TypeScript config not found: ${configPath}`,
          'error'
        );
        process.exit(1);
      }

      try {
        const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));

        loggerManager.printLine(
          `${chalk.green('✓')} TypeScript config is valid`,
          'info'
        );

        if (config.compilerOptions && !options.quiet) {
          const opts = config.compilerOptions;
          loggerManager.printLine(`${chalk.dim('Configuration:')}`, 'info');
          loggerManager.printLine(`  Target: ${opts.target || 'ES5'}`, 'info');
          loggerManager.printLine(
            `  Module: ${opts.module || 'CommonJS'}`,
            'info'
          );
          loggerManager.printLine(`  Strict: ${opts.strict || false}`, 'info');
          loggerManager.printLine(
            `  Source Maps: ${opts.sourceMap || false}`,
            'info'
          );
          loggerManager.printLine(
            `  Skip Lib Check: ${opts.skipLibCheck || false}`,
            'info'
          );
        }
      } catch (error) {
        loggerManager.printLine(
          `${chalk.red('✖')} Invalid TypeScript config: ${(error as Error).message}`,
          'error'
        );
        process.exit(1);
      }
    });

  // Performance info command
  program
    .command('dev:info')
    .description('Show development server information')
    .action(() => {
      const path = require('path');
      const fs = require('fs');
      const os = require('os');

      console.log('');
      loggerManager.printLine(
        `${chalk.blue('⚡')} ${chalk.bold('neex dev')} - Development Server Info`,
        'info'
      );
      console.log('');

      loggerManager.printLine(`${chalk.dim('System:')}`, 'info');
      loggerManager.printLine(
        `  Platform: ${os.platform()} ${os.arch()}`,
        'info'
      );
      loggerManager.printLine(`  Node.js: ${process.version}`, 'info');
      loggerManager.printLine(
        `  Memory: ${Math.round(process.memoryUsage().heapUsed / 1024 / 1024)}MB used`,
        'info'
      );

      console.log('');
      loggerManager.printLine(`${chalk.dim('Features:')}`, 'info');
      loggerManager.printLine(
        `  ${chalk.green('✓')} Ultra-fast TypeScript compilation`,
        'info'
      );
      loggerManager.printLine(
        `  ${chalk.green('✓')} Intelligent module caching`,
        'info'
      );
      loggerManager.printLine(
        `  ${chalk.green('✓')} Hot reload with dependency tracking`,
        'info'
      );
      loggerManager.printLine(
        `  ${chalk.green('✓')} Source map support`,
        'info'
      );
      loggerManager.printLine(
        `  ${chalk.green('✓')} Memory-optimized processing`,
        'info'
      );

      const tsConfigExists = fs.existsSync('tsconfig.json');
      const packageJsonExists = fs.existsSync('package.json');

      console.log('');
      loggerManager.printLine(`${chalk.dim('Project:')}`, 'info');
      loggerManager.printLine(
        `  TypeScript Config: ${tsConfigExists ? chalk.green('✓') : chalk.red('✖')}`,
        'info'
      );
      loggerManager.printLine(
        `  Package.json: ${packageJsonExists ? chalk.green('✓') : chalk.red('✖')}`,
        'info'
      );

      if (packageJsonExists) {
        try {
          const pkg = JSON.parse(fs.readFileSync('package.json', 'utf8'));
          if (pkg.name) {
            loggerManager.printLine(`  Name: ${pkg.name}`, 'info');
          }
          if (pkg.version) {
            loggerManager.printLine(`  Version: ${pkg.version}`, 'info');
          }
        } catch (error) {
          // Ignore package.json parsing errors
        }
      }

      console.log('');
    });

  // Cleanup function is no longer needed here as it's handled within the command
  const cleanupDev = () => {};

  return { cleanupDev };
}
