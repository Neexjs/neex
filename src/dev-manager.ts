// src/dev-manager.ts - Development server manager with hot reloading
import { spawn, ChildProcess } from 'child_process';
import { watch } from 'chokidar';
import { loggerManager } from './logger-manager.js';
import chalk from 'chalk';
import figures from 'figures';
import path from 'path';
import fs from 'fs';
import { debounce } from 'lodash';

async function isCommandAvailable(command: string): Promise<boolean> {
    try {
        const { exec } = await import('child_process');
        return new Promise((resolve) => {
            exec(`command -v ${command}`, (error) => {
                resolve(!error);
            });
        });
    } catch (e) {
        return false;
    }
}

export interface DevOptions {
    file: string;
    watch: string[];
    ignore: string[];
    extensions: string[];
    delay: number;
    color: boolean;
    quiet: boolean;
    verbose: boolean;
    clearConsole: boolean;
    inspect: boolean;
    inspectBrk: boolean;
    envFile: string;
    execCommand?: string;
}

export class DevManager {
    private options: DevOptions;
    private process: ChildProcess | null = null;
    private watcher: any = null;
    private isRestarting = false;
    private restartCount = 0;
    private startTime: Date | null = null;
    private debouncedRestart: () => void;

    constructor(options: DevOptions) {
        this.options = options;
        this.debouncedRestart = debounce(this.restart.bind(this), options.delay);
    }

    private loadEnvFile(): void {
        if (this.options.envFile && fs.existsSync(this.options.envFile)) {
            try {
                const envContent = fs.readFileSync(this.options.envFile, 'utf8');
                const lines = envContent.split('\n');
                
                for (const line of lines) {
                    const trimmed = line.trim();
                    if (trimmed && !trimmed.startsWith('#')) {
                        const [key, ...values] = trimmed.split('=');
                        if (key && values.length > 0) {
                            process.env[key.trim()] = values.join('=').trim();
                        }
                    }
                }
                
                if (this.options.verbose) {
                    loggerManager.printLine(`Loaded environment variables from ${this.options.envFile}`, 'info');
                }
            } catch (error) {
                loggerManager.printLine(`Failed to load environment file ${this.options.envFile}: ${(error as Error).message}`, 'warn');
            }
        }
    }

    private async getExecuteCommand(): Promise<{ command: string; args: string[] }> {
        if (this.options.execCommand) {
            const parts = this.options.execCommand.split(' ');
            return { command: parts[0], args: [...parts.slice(1), this.options.file] };
        }

        // Default to tsx for TypeScript files
        const tsxExists = await isCommandAvailable('tsx');
        if (!tsxExists) {
            throw new Error('`tsx` command not found. Please install `tsx`');
        }

        const args = [this.options.file];
        
        if (this.options.inspect) {
            args.unshift('--inspect');
        }
        
        if (this.options.inspectBrk) {
            args.unshift('--inspect-brk');
        }

        return { command: 'tsx', args };
    }

    private clearConsole(): void {
        if (this.options.clearConsole && process.stdout.isTTY) {
            process.stdout.write('\x1b[2J\x1b[0f');
        }
    }

    private async startProcess(): Promise<void> {
        if (this.process) {
            return;
        }

        this.loadEnvFile();
        
        const { command, args } = await this.getExecuteCommand();
        
        if (this.options.verbose) {
            loggerManager.printLine(`Executing: ${command} ${args.join(' ')}`, 'info');
        }

        this.process = spawn(command, args, {
            stdio: ['ignore', 'pipe', 'pipe'],
            shell: false,
            env: {
                ...process.env,
                NODE_ENV: process.env.NODE_ENV || 'development',
                FORCE_COLOR: this.options.color ? '1' : '0'
            },
            detached: true
        });

        this.startTime = new Date();
        this.restartCount++;

        if (!this.options.quiet) {
            const timestamp = new Date().toLocaleTimeString();
            loggerManager.printLine(
                `${chalk.green(figures.play)} Started ${chalk.cyan(this.options.file)} ${chalk.dim(`(${timestamp})`)}`,
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
            loggerManager.printLine(`Process error: ${error.message}`, 'error');
        });

        this.process.on('exit', (code, signal) => {
            if (this.process) {
                this.process = null;
                
                if (!this.isRestarting) {
                    if (code !== 0) {
                        const duration = this.startTime ? Date.now() - this.startTime.getTime() : 0;
                        loggerManager.printLine(
                            `${chalk.red(figures.cross)} Process exited with code ${code} after ${duration}ms`,
                            'error'
                        );
                    }
                }
            }
        });
    }

    private async stopProcess(): Promise<void> {
        if (!this.process) {
            return;
        }

        return new Promise<void>((resolve) => {
            if (!this.process) {
                resolve();
                return;
            }

            const proc = this.process;
            this.process = null;

            const cleanup = () => {
                if (!this.options.quiet) {
                    loggerManager.printLine(`${chalk.yellow(figures.square)} Stopped process`, 'info');
                }
                resolve();
            };

            proc.on('exit', cleanup);
            proc.on('error', cleanup);

            try {
                if (proc.pid) {
                    // Kill process group
                    process.kill(-proc.pid, 'SIGTERM');
                    
                    // Fallback after timeout
                    setTimeout(() => {
                        if (proc.pid && !proc.killed) {
                            try {
                                process.kill(-proc.pid, 'SIGKILL');
                            } catch (e) {
                                // Ignore
                            }
                        }
                    }, 5000);
                }
            } catch (error) {
                // Process might already be dead
                cleanup();
            }
        });
    }

    private async restart(): Promise<void> {
        if (this.isRestarting) {
            return;
        }

        this.isRestarting = true;
        
        if (this.options.clearConsole) {
            this.clearConsole();
        }

        if (!this.options.quiet) {
            loggerManager.printLine(`${chalk.yellow(figures.arrowRight)} Restarting due to changes...`, 'info');
        }

        await this.stopProcess();
        await this.startProcess();
        
        this.isRestarting = false;
    }

    private setupWatcher(): void {
        const watchPatterns = this.options.watch;
        const ignored = [
            '**/node_modules/**',
            '**/.git/**',
            '**/dist/**',
            '**/build/**',
            '**/*.log',
            ...this.options.ignore.map(pattern => `**/${pattern}/**`)
        ];

        this.watcher = watch(watchPatterns, {
            ignored,
            ignoreInitial: true,
            followSymlinks: false,
            usePolling: false,
            atomic: 300
        });

        this.watcher.on('change', (filePath: string) => {
            if (this.options.verbose) {
                loggerManager.printLine(`File changed: ${path.relative(process.cwd(), filePath)}`, 'info');
            }
            this.debouncedRestart();
        });

        this.watcher.on('add', (filePath: string) => {
            if (this.options.verbose) {
                loggerManager.printLine(`File added: ${path.relative(process.cwd(), filePath)}`, 'info');
            }
            this.debouncedRestart();
        });

        this.watcher.on('unlink', (filePath: string) => {
            if (this.options.verbose) {
                loggerManager.printLine(`File removed: ${path.relative(process.cwd(), filePath)}`, 'info');
            }
            this.debouncedRestart();
        });

        this.watcher.on('error', (error: Error) => {
            loggerManager.printLine(`Watcher error: ${error.message}`, 'error');
        });

        if (this.options.verbose) {
            loggerManager.printLine(`Watching: ${watchPatterns.join(', ')}`, 'info');
            loggerManager.printLine(`Ignoring: ${ignored.join(', ')}`, 'info');
        }
    }

    public async start(): Promise<void> {
        // Check if target file exists
        if (!fs.existsSync(this.options.file)) {
            throw new Error(`Target file not found: ${this.options.file}`);
        }

        loggerManager.printLine(`${chalk.blue(figures.info)} Starting development server...`, 'info');
        
        // Show configuration in verbose mode
        if (this.options.verbose) {
            loggerManager.printLine(`Target file: ${this.options.file}`, 'info');
            loggerManager.printLine(`Watch patterns: ${this.options.watch.join(', ')}`, 'info');
            loggerManager.printLine(`Restart delay: ${this.options.delay}ms`, 'info');
        }

        this.setupWatcher();
        await this.startProcess();

        loggerManager.printLine(
            `${chalk.green(figures.tick)} Development server started. Watching for changes...`,
            'info'
        );
    }

    public async stop(): Promise<void> {
        loggerManager.printLine(`${chalk.yellow(figures.warning)} Stopping development server...`, 'info');
        
        if (this.watcher) {
            await this.watcher.close();
            this.watcher = null;
        }

        await this.stopProcess();
        
        if (this.restartCount > 0) {
            loggerManager.printLine(
                `${chalk.blue(figures.info)} Development server stopped after ${this.restartCount} restart(s)`,
                'info'
            );
        }
    }
}