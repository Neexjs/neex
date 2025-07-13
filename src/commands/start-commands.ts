// src/commands/start-commands.ts - Fixed production start command
import { Command } from 'commander';
import { StartManager } from '../start-manager.js';
import { loggerManager } from '../logger-manager.js';
import chalk from 'chalk';
import figures from 'figures';
import path from 'path';
import fs from 'fs';
import os from 'os';

export function addStartCommands(program: Command): { cleanupStart: () => void } {
    let startManager: StartManager | null = null;

    // Production start command
    program
        .command('start [file]')
        .description('Start production application')
        .option('-d, --dir <directory>', 'Working directory', process.cwd())
        .option('-e, --env <file>', 'Environment file to load', '.env')
        .option('-p, --port <port>', 'Port number', parseInt)
        .option('-w, --workers <count>', 'Number of worker processes', parseInt, 1)
        .option('-v, --verbose', 'Verbose output')
        .option('--watch', 'Watch for changes and restart (development mode)')
        .option('--no-health', 'Disable health check endpoint')
        .option('--health-port <port>', 'Health check port', parseInt, 3001)
        .option('--max-memory <limit>', 'Maximum memory before restart (e.g., 1G)')
        .option('--graceful-timeout <ms>', 'Graceful shutdown timeout (ms)', parseInt, 30000)
        .option('--inspect', 'Enable Node.js inspector')
        .option('--inspect-brk', 'Enable Node.js inspector with break')
        .option('--node-args <args>', 'Additional Node.js arguments')
        .action(async (file, options) => {
            try {
                const targetFile = file || 'dist/server.js';
                let resolvedFile = path.resolve(options.dir, targetFile);
                
                // Auto-detect main file if not found
                if (!fs.existsSync(resolvedFile)) {
                    const packageJsonPath = path.join(options.dir, 'package.json');
                    if (fs.existsSync(packageJsonPath)) {
                        try {
                            const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
                            const mainFile = packageJson.main || 'index.js';
                            const alternativeFile = path.resolve(options.dir, mainFile);
                            
                            if (fs.existsSync(alternativeFile)) {
                                resolvedFile = alternativeFile;
                                if (options.verbose) {
                                    loggerManager.printLine(`Using main file: ${mainFile}`, 'info');
                                }
                            } else {
                                // Try common locations
                                const commonLocations = [
                                    'dist/server.js',
                                    'dist/app.js',
                                    'dist/index.js',
                                    'build/server.js',
                                    'build/app.js',
                                    'build/index.js',
                                    'server.js',
                                    'app.js',
                                    'index.js'
                                ];
                                
                                let found = false;
                                for (const location of commonLocations) {
                                    const testPath = path.resolve(options.dir, location);
                                    if (fs.existsSync(testPath)) {
                                        resolvedFile = testPath;
                                        found = true;
                                        if (options.verbose) {
                                            loggerManager.printLine(`Found application file: ${location}`, 'info');
                                        }
                                        break;
                                    }
                                }
                                
                                if (!found) {
                                    throw new Error(`Application file not found. Tried: ${targetFile}, ${mainFile}, and common locations.`);
                                }
                            }
                        } catch (parseError) {
                            throw new Error(`Failed to parse package.json: ${(parseError as Error).message}`);
                        }
                    } else {
                        throw new Error(`Application file not found: ${resolvedFile}`);
                    }
                }

                // Environment detection
                const isDevelopment = options.watch || process.env.NODE_ENV === 'development';
                const isProduction = !isDevelopment;
                
                const healthCheck = options.health !== false;
                const defaultPort = parseInt(process.env.PORT || '8000');
                const port = options.port || defaultPort;

                // Set NODE_ENV if not already set
                if (!process.env.NODE_ENV) {
                    process.env.NODE_ENV = isProduction ? 'production' : 'development';
                }

                // Startup logging
                const mode = isDevelopment ? 'development' : 'production';
                const workerText = options.workers === 1 ? 'worker' : 'workers';
                const clusterInfo = `(${options.workers} ${workerText})`;
                loggerManager.printLine(
                    `${chalk.green(figures.play)} Starting ${mode} server ${clusterInfo}`,
                    'info'
                );

                if (options.verbose) {
                    loggerManager.printLine(`File: ${path.relative(process.cwd(), resolvedFile)}`);
                    loggerManager.printLine(`Working Directory: ${options.dir}`);
                    loggerManager.printLine(`Environment: ${process.env.NODE_ENV}`);
                    loggerManager.printLine(`Port: ${port}`);
                    loggerManager.printLine(`Workers: ${options.workers}`);
                    if (healthCheck) {
                        loggerManager.printLine(`Health Check: http://localhost:${options.healthPort}/health`);
                    }
                }

                startManager = new StartManager({
                    file: resolvedFile,
                    workingDir: options.dir,
                    envFile: options.env,
                    port: options.port,
                    workers: options.workers,
                    memoryLimit: options.maxMemory,
                    logLevel: options.verbose ? 'verbose' : 'info',
                    color: true, // Assuming color is always enabled
                    verbose: options.verbose,
                    watch: options.watch,
                    maxCrashes: 5, // Default max crashes
                    restartDelay: 2000, // Default restart delay
                    healthCheck,
                    healthPort: options.healthPort,
                    gracefulTimeout: options.gracefulTimeout,
                    inspect: options.inspect,
                    inspectBrk: options.inspectBrk,
                    nodeArgs: options.nodeArgs
                });

                await startManager.start();
                
            } catch (error: unknown) {
                if (error instanceof Error) {
                    loggerManager.printLine(`${chalk.red(figures.cross)} ${error.message}`, 'error');
                } else {
                    loggerManager.printLine(`${chalk.red(figures.cross)} Startup failed`, 'error');
                }
                process.exit(1);
            }
        });

    // Cleanup function
    const cleanupStart = async () => {
        if (startManager) {
            try {
                await startManager.stop();
                startManager = null;
            } catch (error) {
                if (process.env.VERBOSE) {
                    console.error('Cleanup error:', error);
                }
            }
        }
    };

    // Signal handling
    const handleExit = (signal: string) => {
        if (startManager) {
            console.log(`\n${chalk.yellow(figures.warning)} Received ${signal}, shutting down gracefully...`);
            cleanupStart().then(() => {
                process.exit(0);
            }).catch(() => {
                process.exit(1);
            });
        } else {
            process.exit(0);
        }
    };

    process.on('SIGINT', () => handleExit('SIGINT'));
    process.on('SIGTERM', () => handleExit('SIGTERM'));
    process.on('SIGUSR2', () => handleExit('SIGUSR2'));

    process.on('uncaughtException', (error) => {
        console.error(`${chalk.red(figures.cross)} Uncaught Exception:`, error);
        cleanupStart().then(() => {
            process.exit(1);
        });
    });

    process.on('unhandledRejection', (reason, promise) => {
        console.error(`${chalk.red(figures.cross)} Unhandled Rejection at:`, promise, 'reason:', reason);
        cleanupStart().then(() => {
            process.exit(1);
        });
    });

    return { cleanupStart };
}