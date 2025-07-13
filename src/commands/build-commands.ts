// src/commands/build-commands.ts - Build commands for TypeScript projects
import { Command } from 'commander';
import { BuildManager } from '../build-manager.js';
import { loggerManager } from '../logger-manager.js';
import chalk from 'chalk';
import figures from 'figures';

export function addBuildCommands(program: Command): { cleanupBuild: () => void } {
    let buildManager: BuildManager | null = null;

    // Build command for TypeScript projects
    program
        .command('build [source]')
        .description('Build TypeScript project for production (default: src)')
        .option('-o, --output <dir>', 'Output directory', 'dist')
        .option('-w, --watch', 'Watch mode for continuous building')
        .option('-c, --clean', 'Clean output directory before build', true)
        .option('-s, --sourcemap', 'Generate source maps')
        .option('-t, --target <target>', 'TypeScript target (es2020, es2022, etc.)', 'es2020')
        .option('-f, --format <format>', 'Output format (cjs, esm)', 'cjs')
        .option('--tsconfig <file>', 'TypeScript config file', 'tsconfig.json')
        .option('-v, --verbose', 'Verbose output')
        .option('-q, --quiet', 'Quiet output')
        .option('--no-color', 'Disable colored output')
        .option('--analyze', 'Analyze bundle size')
        .action(async (source, options) => {
            try {
                const sourceDir = source || 'src';
                
                if (!options.quiet) {
                    loggerManager.printLine(`${chalk.blue(figures.info)} Building TypeScript project from ${chalk.cyan(sourceDir)}`, 'info');
                }

                buildManager = new BuildManager({
                    source: sourceDir,
                    output: options.output,
                    watch: options.watch,
                    clean: options.clean,
                    minify: false, // TSC doesn't handle minification
                    sourcemap: options.sourcemap,
                    target: options.target,
                    format: options.format,
                    bundle: false, // TSC doesn't bundle
                    external: [], // Not applicable for TSC
                    tsconfig: options.tsconfig,
                    verbose: options.verbose,
                    quiet: options.quiet,
                    color: options.color,
                    analyze: options.analyze
                });

                await buildManager.build();
                
                // If not in watch mode, show completion message
                if (!options.watch && !options.quiet) {
                    loggerManager.printLine(`${chalk.green(figures.tick)} Build completed successfully`, 'info');
                }
                
            } catch (error: unknown) {
                if (error instanceof Error) {
                    loggerManager.printLine(`${chalk.red(figures.cross)} Build failed: ${error.message}`, 'error');
                } else {
                    loggerManager.printLine(`${chalk.red(figures.cross)} An unknown build error occurred`, 'error');
                }
                process.exit(1);
            }
        });

    // Add a quick build command without options
    program
        .command('compile [source]')
        .alias('tsc')
        .description('Quick TypeScript compilation (alias for build)')
        .action(async (source) => {
            try {
                const sourceDir = source || 'src';
                
                loggerManager.printLine(`${chalk.blue(figures.info)} Compiling TypeScript...`, 'info');

                buildManager = new BuildManager({
                    source: sourceDir,
                    output: 'dist',
                    watch: false,
                    clean: true,
                    minify: false,
                    sourcemap: false,
                    target: 'es2020',
                    format: 'cjs',
                    bundle: false,
                    external: [],
                    tsconfig: 'tsconfig.json',
                    verbose: false,
                    quiet: false,
                    color: true,
                    analyze: false
                });

                await buildManager.build();
                loggerManager.printLine(`${chalk.green(figures.tick)} Compilation completed`, 'info');
                
            } catch (error: unknown) {
                if (error instanceof Error) {
                    loggerManager.printLine(`${chalk.red(figures.cross)} Compilation failed: ${error.message}`, 'error');
                } else {
                    loggerManager.printLine(`${chalk.red(figures.cross)} An unknown compilation error occurred`, 'error');
                }
                process.exit(1);
            }
        });

    // Cleanup function
    const cleanupBuild = async () => {
        if (buildManager) {
            try {
                await buildManager.stop();
                buildManager = null;
            } catch (error) {
                // Ignore cleanup errors
            }
        }
    };

    // Handle process termination
    const handleExit = (signal: string) => {
        if (buildManager) {
            loggerManager.printLine(`\n${chalk.yellow(figures.warning)} Received ${signal}, stopping build process...`, 'info');
            cleanupBuild().then(() => {
                process.exit(0);
            }).catch(() => {
                process.exit(1);
            });
        } else {
            process.exit(0);
        }
    };

    // Register signal handlers
    process.on('SIGINT', () => handleExit('SIGINT'));
    process.on('SIGTERM', () => handleExit('SIGTERM'));
    process.on('exit', () => {
        if (buildManager) {
            cleanupBuild();
        }
    });

    return { cleanupBuild };
}