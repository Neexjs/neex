import { Command } from 'commander';
import { StartManager, StartOptions } from '../start-manager.js';
import { loggerManager } from '../logger-manager.js';
import path from 'path';
import fs from 'fs';

export function addStartCommands(program: Command) {
    let manager: StartManager | null = null;

    const startCommand = program
        .command('start [entry]')
        .description('Start a production-ready application')
        .option('-n, --name <name>', 'Application name')
        .option('-w, --watch', 'Watch for file changes and restart the application', false)
        .option('--quiet', 'Suppress all output', false)
        .option('--color', 'Force color output', true)
        .option('--verbose', 'Enable verbose logging', false)
        .option('--node-args <args>', 'Arguments to pass to the node process', (value) => value.split(' '), [])
        .action(async (entry, options) => {
            let entryPoint = entry;

            if (!entryPoint) {
                const defaultPath = path.join(process.cwd(), 'dist', 'index.js');
                if (fs.existsSync(defaultPath)) {
                    entryPoint = defaultPath;
                } else {
                    loggerManager.printLine('Entry file not found. Please specify an entry file or build the project first.', 'error');
                    process.exit(1);
                }
            }

            const startOptions: StartOptions = {
                entry: entryPoint,
                name: options.name,
                watch: options.watch,
                quiet: options.quiet,
                color: options.color,
                verbose: options.verbose,
                nodeArgs: options.nodeArgs,
            };

            manager = new StartManager(startOptions);

            try {
                await manager.start();
            } catch (error) {
                loggerManager.printLine((error as Error).message, 'error');
                process.exit(1);
            }
        });

    return {
        cleanupStart: async () => {
            if (manager) {
                await manager.stop();
            }
        },
    };
}