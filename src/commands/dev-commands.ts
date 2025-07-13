// src/commands/dev-commands.ts - Enhanced development commands with TypeScript compilation
import { Command } from 'commander';
import { DevManager } from '../dev-manager.js';
import { loggerManager } from '../logger-manager.js';
import chalk from 'chalk';
import figures from 'figures';

export function addDevCommands(program: Command): { cleanupDev: () => void } {
    let devManager: DevManager | null = null;

    // Enhanced dev command with built-in TypeScript compilation
    program
        .command('dev [file]')
        .description('Start TypeScript development server with hot reloading (default: src/index.ts)')
        .option('-w, --watch <patterns>', 'Watch additional patterns (comma-separated)', 'src/**/*')
        .option('-i, --ignore <patterns>', 'Ignore patterns (comma-separated)', 'node_modules,dist,build,.git,coverage')
        .option('-e, --ext <extensions>', 'File extensions to watch (comma-separated)', 'ts,tsx,js,jsx,json')
        .option('-d, --delay <ms>', 'Delay before restart (ms)', parseInt, 800)
        .option('-c, --no-color', 'Disable colored output')
        .option('-q, --quiet', 'Reduce output verbosity')
        .option('-v, --verbose', 'Verbose output')
        .option('--no-clear', 'Don\'t clear console on restart')
        .option('--inspect', 'Enable Node.js inspector')
        .option('--inspect-brk', 'Enable Node.js inspector with break')
        .option('--env <file>', 'Load environment variables from file', '.env')
        .option('--exec <command>', 'Command to execute instead of built-in TypeScript compilation')
        .option('--tsconfig <path>', 'Path to TypeScript configuration file')
        .option('--no-source-maps', 'Disable source maps')
        .option('--transpile-only', 'Skip type checking for faster compilation')
        .option('--node-args <args>', 'Additional Node.js arguments (comma-separated)', '')
        .action(async (file, options) => {
            try {
                const targetFile = file || 'src/index.ts';
                
                if (!options.quiet) {
                    loggerManager.printLine(`${chalk.blue(figures.info)} Starting ${chalk.cyan('neex dev')} for ${chalk.cyan(targetFile)}`, 'info');
                    
                    if (options.verbose) {
                        loggerManager.printLine(`${chalk.dim('Features:')}`, 'info');
                        loggerManager.printLine(`  ${chalk.green('✓')} Built-in TypeScript compilation`, 'info');
                        loggerManager.printLine(`  ${chalk.green('✓')} Hot reloading with intelligent caching`, 'info');
                        loggerManager.printLine(`  ${chalk.green('✓')} Source map support`, 'info');
                        loggerManager.printLine(`  ${chalk.green('✓')} Fast transpilation mode`, 'info');
                        loggerManager.printLine(`  ${chalk.green('✓')} Dependency tracking`, 'info');
                    }
                }

                devManager = new DevManager({
                    file: targetFile,
                    watch: options.watch.split(',').map((p: string) => p.trim()),
                    ignore: options.ignore.split(',').map((p: string) => p.trim()),
                    extensions: options.ext.split(',').map((e: string) => e.trim()),
                    delay: options.delay,
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
                    nodeArgs: options.nodeArgs ? options.nodeArgs.split(',').map((arg: string) => arg.trim()) : []
                });

                await devManager.start();
                
            } catch (error: unknown) {
                if (error instanceof Error) {
                    loggerManager.printLine(`${chalk.red(figures.cross)} Development server error: ${error.message}`, 'error');
                } else {
                    loggerManager.printLine(`${chalk.red(figures.cross)} An unknown development server error occurred`, 'error');
                }
                process.exit(1);
            }
        });

    // Additional helper commands
    program
        .command('dev:clean')
        .description('Clean development server cache and temporary files')
        .action(() => {
            const path = require('path');
            const fs = require('fs');
            
            const tempDir = path.join(process.cwd(), '.neex-temp');
            if (fs.existsSync(tempDir)) {
                fs.rmSync(tempDir, { recursive: true, force: true });
                loggerManager.printLine(`${chalk.green(figures.tick)} Cleaned development cache`, 'info');
            } else {
                loggerManager.printLine(`${chalk.yellow(figures.info)} No cache to clean`, 'info');
            }
        });

    program
        .command('dev:check')
        .description('Check TypeScript configuration and dependencies')
        .option('--tsconfig <path>', 'Path to TypeScript configuration file')
        .action((options) => {
            const path = require('path');
            const fs = require('fs');
            
            const configPath = options.tsconfig || 'tsconfig.json';
            
            if (!fs.existsSync(configPath)) {
                loggerManager.printLine(`${chalk.red(figures.cross)} TypeScript config not found: ${configPath}`, 'error');
                process.exit(1);
            }

            try {
                const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
                loggerManager.printLine(`${chalk.green(figures.tick)} TypeScript config is valid`, 'info');
                
                if (config.compilerOptions) {
                    loggerManager.printLine(`${chalk.dim('Compiler Options:')}`, 'info');
                    loggerManager.printLine(`  Target: ${config.compilerOptions.target || 'default'}`, 'info');
                    loggerManager.printLine(`  Module: ${config.compilerOptions.module || 'default'}`, 'info');
                    loggerManager.printLine(`  Strict: ${config.compilerOptions.strict || 'false'}`, 'info');
                    loggerManager.printLine(`  Source Maps: ${config.compilerOptions.sourceMap || 'false'}`, 'info');
                }
            } catch (error) {
                loggerManager.printLine(`${chalk.red(figures.cross)} Invalid TypeScript config: ${(error as Error).message}`, 'error');
                process.exit(1);
            }
        });

    // Cleanup function
    const cleanupDev = () => {
        if (devManager) {
            devManager.stop();
            devManager = null;
        }
    };

    // Handle process termination
    process.on('SIGINT', () => {
        if (devManager) {
            loggerManager.printLine(`\n${chalk.yellow(figures.warning)} Received SIGINT, shutting down gracefully...`, 'info');
            cleanupDev();
            process.exit(0);
        }
    });

    process.on('SIGTERM', () => {
        if (devManager) {
            loggerManager.printLine(`\n${chalk.yellow(figures.warning)} Received SIGTERM, shutting down gracefully...`, 'info');
            cleanupDev();
            process.exit(0);
        }
    });

    return { cleanupDev };
}