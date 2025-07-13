// src/start-manager.ts - Production application runner
import { spawn, ChildProcess } from 'child_process';
import { loggerManager } from './logger-manager.js';
import chalk from 'chalk';
import figures from 'figures';
import fs from 'fs';
import path from 'path';

export interface StartOptions {
    entry: string;
    name?: string;
    watch?: boolean;
    quiet?: boolean;
    color?: boolean;
    verbose?: boolean;
    nodeArgs?: string[];
}

export class StartManager {
    private options: StartOptions;
    private process: ChildProcess | null = null;
    private isStopping = false;

    constructor(options: StartOptions) {
        this.options = options;
    }

    private async startProcess(): Promise<void> {
        if (this.process) {
            return;
        }

        const nodeArgs = this.options.nodeArgs || [];
        const args = [...nodeArgs, this.options.entry];

        if (this.options.verbose) {
            loggerManager.printLine(`Executing: node ${args.join(' ')}`, 'info');
        }

        this.process = spawn('node', args, {
            stdio: ['ignore', 'pipe', 'pipe'],
            shell: false,
            env: {
                ...process.env,
                NODE_ENV: 'production',
                FORCE_COLOR: this.options.color ? '1' : '0'
            },
            detached: true
        });

        const appName = this.options.name || path.basename(this.options.entry);

        if (!this.options.quiet) {
            loggerManager.printLine(
                `${chalk.green(figures.play)} Starting ${chalk.cyan(appName)} in production mode...`,
                'info'
            );
        }

        this.process.stdout?.on('data', (data) => {
            if (!this.options.quiet) {
                process.stdout.write(data);
            }
        });

        this.process.stderr?.on('data', (data) => {
            if (!this.options.quiet) {
                process.stderr.write(data);
            }
        });

        this.process.on('error', (error) => {
            loggerManager.printLine(`Application error: ${error.message}`, 'error');
        });

        this.process.on('exit', (code) => {
            this.process = null;
            if (!this.isStopping) {
                loggerManager.printLine(
                    `${chalk.red(figures.cross)} Application ${appName} exited with code ${code}`,
                    'error'
                );
                if (this.options.watch) {
                    loggerManager.printLine(`${chalk.yellow(figures.arrowRight)} Restarting application...`, 'info');
                    this.startProcess();
                }
            }
        });
    }

    public async start(): Promise<void> {
        if (!fs.existsSync(this.options.entry)) {
            throw new Error(`Entry file not found: ${this.options.entry}`);
        }

        await this.startProcess();

        if (!this.options.quiet) {
            loggerManager.printLine(
                `${chalk.green(figures.tick)} Application is running.`, 'info'
            );
        }
    }

    public async stop(): Promise<void> {
        this.isStopping = true;
        const proc = this.process;

        if (!proc) {
            return;
        }

        this.process = null;

        return new Promise<void>((resolve) => {
            const appName = this.options.name || path.basename(this.options.entry);
            loggerManager.printLine(`${chalk.yellow(figures.warning)} Stopping application ${appName}...`, 'info');

            proc.on('exit', () => {
                loggerManager.printLine(`${chalk.yellow(figures.square)} Application stopped.`, 'info');
                resolve();
            });

            proc.on('error', () => {
                // Handle errors during shutdown, e.g., if the process is already gone
                loggerManager.printLine(`${chalk.yellow(figures.square)} Application stopped.`, 'info');
                resolve();
            });

            try {
                if (proc.pid) {
                    const pid = proc.pid;
                    // Kill the entire process group
                    process.kill(-pid, 'SIGTERM');

                    // Set a timeout to force kill if it doesn't terminate gracefully
                    setTimeout(() => {
                        if (!proc.killed) {
                            try {
                                process.kill(-pid, 'SIGKILL');
                            } catch (e) {
                                // Ignore errors if the process is already gone
                            }
                        }
                    }, 5000).unref(); // .unref() allows the main process to exit if this is the only thing running
                }
            } catch (e) {
                // This can happen if the process is already dead
                resolve();
            }
        });
    }
}