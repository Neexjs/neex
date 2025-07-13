// src/commands/dev-commands.ts - Development commands with hot reloading
import { Command } from 'commander';
import { DevManager } from '../dev-manager.js';
import { loggerManager } from '../logger-manager.js';
import chalk from 'chalk';
import figures from 'figures';

export function addDevCommands(program: Command): { cleanupDev: () => void } {
    let devManager: DevManager | null = null;

    // Dev command for hot reloading development
    program
        .command('dev [file]')
        .description('Start development server with hot reloading (default: src/index.ts)')
        .option('-w, --watch <patterns>', 'Watch additional patterns (comma-separated)', 'src/**/*')
        .option('-i, --ignore <patterns>', 'Ignore patterns (comma-separated)', 'node_modules,dist,build,.git')
        .option('-e, --ext <extensions>', 'File extensions to watch (comma-separated)', 'ts,js,json')
        .option('-d, --delay <ms>', 'Delay before restart (ms)', parseInt, 1000)
        .option('-c, --no-color', 'Disable colored output')
        .option('-q, --quiet', 'Reduce output verbosity')
        .option('-v, --verbose', 'Verbose output')
        .option('--no-clear', 'Don\'t clear console on restart')
        .option('--inspect', 'Enable Node.js inspector')
        .option('--inspect-brk', 'Enable Node.js inspector with break')
        .option('--env <file>', 'Load environment variables from file', '.env')
        .option('--exec <command>', 'Command to execute instead of tsx')
        .action(async (file, options) => {
            try {
                const targetFile = file || 'src/index.ts';
                
                loggerManager.printLine(`Starting development server for ${chalk.cyan(targetFile)}`, 'info');
                
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
                    execCommand: options.exec
                });

                await devManager.start();
                
            } catch (error: unknown) {
                if (error instanceof Error) {
                    loggerManager.printLine(`Development server error: ${error.message}`, 'error');
                } else {
                    loggerManager.printLine('An unknown development server error occurred', 'error');
                }
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

    return { cleanupDev };
}