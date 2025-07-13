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
        .option('-c, --clean', 'Clean output directory before build')
        .option('-m, --minify', 'Minify output')
        .option('-s, --sourcemap', 'Generate source maps')
        .option('-t, --target <target>', 'TypeScript target (es2020, es2022, etc.)', 'es2020')
        .option('-f, --format <format>', 'Output format (cjs, esm)', 'cjs')
        .option('--no-bundle', 'Don\'t bundle, just compile')
        .option('--external <packages>', 'External packages (comma-separated)')
        .option('--tsconfig <file>', 'TypeScript config file', 'tsconfig.json')
        .option('-v, --verbose', 'Verbose output')
        .option('-q, --quiet', 'Quiet output')
        .option('--no-color', 'Disable colored output')
        .option('--analyze', 'Analyze bundle size')
        .action(async (source, options) => {
            try {
                const sourceDir = source || 'src';
                
                loggerManager.printLine(`Building TypeScript project from ${chalk.cyan(sourceDir)}`, 'info');
                
                buildManager = new BuildManager({
                    source: sourceDir,
                    output: options.output,
                    watch: options.watch,
                    clean: options.clean,
                    minify: options.minify,
                    sourcemap: options.sourcemap,
                    target: options.target,
                    format: options.format,
                    bundle: options.bundle,
                    external: options.external ? options.external.split(',').map((p: string) => p.trim()) : [],
                    tsconfig: options.tsconfig,
                    verbose: options.verbose,
                    quiet: options.quiet,
                    color: options.color,
                    analyze: options.analyze
                });

                await buildManager.build();
                
            } catch (error: unknown) {
                if (error instanceof Error) {
                    loggerManager.printLine(`Build error: ${error.message}`, 'error');
                } else {
                    loggerManager.printLine('An unknown build error occurred', 'error');
                }
                process.exit(1);
            }
        });

    // Cleanup function
    const cleanupBuild = () => {
        if (buildManager) {
            buildManager.stop();
            buildManager = null;
        }
    };

    return { cleanupBuild };
}