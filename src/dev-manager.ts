// src/dev-manager.ts - Enhanced TypeScript development server with built-in compilation
import { spawn, ChildProcess } from 'child_process';
import { watch } from 'chokidar';
import { loggerManager } from './logger-manager.js';
import chalk from 'chalk';
import figures from 'figures';
import path from 'path';
import fs from 'fs';
import { debounce } from 'lodash';
import * as ts from 'typescript';
import { createRequire } from 'module';

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
    tsConfig?: string;
    sourceMaps: boolean;
    transpileOnly: boolean;
    nodeArgs: string[];
}

interface CompiledModule {
    code: string;
    map?: string;
    dependencies: Set<string>;
}

export class DevManager {
    private options: DevOptions;
    private process: ChildProcess | null = null;
    private watcher: any = null;
    private isRestarting = false;
    private restartCount = 0;
    private startTime: Date | null = null;
    private debouncedRestart: () => void;
    private moduleCache = new Map<string, CompiledModule>();
    private tsCompilerOptions: ts.CompilerOptions;
    private fileWatcher: Map<string, boolean> = new Map();

    constructor(options: DevOptions) {
        this.options = options;
        this.debouncedRestart = debounce(this.restart.bind(this), options.delay);
        this.tsCompilerOptions = this.loadTsConfig();
    }

    private loadTsConfig(): ts.CompilerOptions {
        const configPath = this.options.tsConfig || 'tsconfig.json';
        const defaultOptions: ts.CompilerOptions = {
            target: ts.ScriptTarget.ES2022,
            module: ts.ModuleKind.CommonJS,
            moduleResolution: ts.ModuleResolutionKind.NodeJs,
            allowJs: true,
            outDir: undefined,
            rootDir: undefined,
            strict: false,
            esModuleInterop: true,
            skipLibCheck: true,
            forceConsistentCasingInFileNames: true,
            resolveJsonModule: true,
            declaration: false,
            sourceMap: this.options.sourceMaps,
            inlineSourceMap: false,
            inlineSources: false,
        };

        if (fs.existsSync(configPath)) {
            try {
                const configFile = ts.readConfigFile(configPath, ts.sys.readFile);
                if (configFile.error) {
                    loggerManager.printLine(`Error reading tsconfig.json: ${configFile.error.messageText}`, 'warn');
                    return defaultOptions;
                }

                const parsedConfig = ts.parseJsonConfigFileContent(
                    configFile.config,
                    ts.sys,
                    path.dirname(configPath)
                );

                if (parsedConfig.errors.length > 0) {
                    loggerManager.printLine(`Error parsing tsconfig.json: ${parsedConfig.errors[0].messageText}`, 'warn');
                    return defaultOptions;
                }

                // Override some options for development
                const options = { ...parsedConfig.options, ...defaultOptions };
                
                if (this.options.verbose) {
                    loggerManager.printLine(`Loaded TypeScript config from ${configPath}`, 'info');
                }

                return options;
            } catch (error) {
                loggerManager.printLine(`Failed to load tsconfig.json: ${(error as Error).message}`, 'warn');
            }
        }

        return defaultOptions;
    }

    private loadEnvFile(): void {
        if (this.options.envFile && fs.existsSync(this.options.envFile)) {
            try {
                const envContent = fs.readFileSync(this.options.envFile, 'utf8');
                const lines = envContent.split('\n');
                let loadedCount = 0;
                
                for (const line of lines) {
                    const trimmed = line.trim();
                    if (trimmed && !trimmed.startsWith('#')) {
                        const [key, ...values] = trimmed.split('=');
                        if (key && values.length > 0) {
                            process.env[key.trim()] = values.join('=').trim().replace(/^["']|["']$/g, '');
                            loadedCount++;
                        }
                    }
                }
                
                if (!this.options.quiet && loadedCount > 0) {
                    loggerManager.printLine(
                        `${chalk.dim(figures.info)} Loaded ${loadedCount} env variable${loadedCount > 1 ? 's' : ''} from ${path.basename(this.options.envFile)}`,
                        'info'
                    );
                } else if (this.options.verbose) {
                    loggerManager.printLine(`${chalk.dim(figures.info)} No env variables found in ${this.options.envFile}`, 'info');
                }
            } catch (error) {
                loggerManager.printLine(`${chalk.yellow(figures.warning)} Failed to load ${this.options.envFile}: ${(error as Error).message}`, 'warn');
            }
        }
    }

    private compileTypeScript(filePath: string): CompiledModule {
        const absolutePath = path.resolve(filePath);
        
        // Check cache first
        const cached = this.moduleCache.get(absolutePath);
        if (cached && !this.options.transpileOnly) {
            return cached;
        }

        try {
            const sourceCode = fs.readFileSync(absolutePath, 'utf8');
            const dependencies = new Set<string>();

            // Extract import/require dependencies
            const importRegex = /(?:import|require)\s*(?:\(.*?\)|.*?from\s+)['"`]([^'"`]+)['"`]/g;
            let match;
            while ((match = importRegex.exec(sourceCode)) !== null) {
                if (!match[1].startsWith('.')) continue;
                const depPath = path.resolve(path.dirname(absolutePath), match[1]);
                dependencies.add(depPath);
            }

            let result: ts.TranspileOutput;
            
            if (this.options.transpileOnly) {
                // Fast transpile without type checking
                result = ts.transpileModule(sourceCode, {
                    compilerOptions: this.tsCompilerOptions,
                    fileName: absolutePath,
                    reportDiagnostics: false
                });
            } else {
                // Full compilation with type checking
                const program = ts.createProgram([absolutePath], this.tsCompilerOptions);
                const sourceFile = program.getSourceFile(absolutePath);
                
                if (!sourceFile) {
                    throw new Error(`Could not load source file: ${absolutePath}`);
                }

                // Check for errors
                const diagnostics = ts.getPreEmitDiagnostics(program, sourceFile);
                if (diagnostics.length > 0) {
                    const errors = diagnostics.map(diagnostic => {
                        const message = ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n');
                        if (diagnostic.file && diagnostic.start !== undefined) {
                            const { line, character } = diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start);
                            return `${path.relative(process.cwd(), diagnostic.file.fileName)}:${line + 1}:${character + 1} - ${message}`;
                        }
                        return message;
                    });
                    
                    loggerManager.printLine(`TypeScript compilation errors:\n${errors.join('\n')}`, 'error');
                    if (!this.options.quiet) {
                        process.exit(1);
                    }
                }

                result = ts.transpileModule(sourceCode, {
                    compilerOptions: this.tsCompilerOptions,
                    fileName: absolutePath,
                    reportDiagnostics: true
                });
            }

            const compiled: CompiledModule = {
                code: result.outputText,
                map: result.sourceMapText,
                dependencies
            };

            this.moduleCache.set(absolutePath, compiled);
            return compiled;

        } catch (error) {
            loggerManager.printLine(`Compilation error in ${filePath}: ${(error as Error).message}`, 'error');
            throw error;
        }
    }

    private createTempFile(compiled: CompiledModule, originalPath: string): string {
        const tempDir = path.join(process.cwd(), '.neex-temp');
        if (!fs.existsSync(tempDir)) {
            fs.mkdirSync(tempDir, { recursive: true });
        }

        const tempFile = path.join(tempDir, `${path.basename(originalPath, path.extname(originalPath))}-${Date.now()}.js`);
        
        let code = compiled.code;
        
        // Handle source maps
        if (compiled.map && this.options.sourceMaps) {
            const mapFile = tempFile + '.map';
            fs.writeFileSync(mapFile, compiled.map);
            code += `\n//# sourceMappingURL=${path.basename(mapFile)}`;
        }

        fs.writeFileSync(tempFile, code);
        return tempFile;
    }

    private cleanupTempFiles(): void {
        const tempDir = path.join(process.cwd(), '.neex-temp');
        if (fs.existsSync(tempDir)) {
            try {
                fs.rmSync(tempDir, { recursive: true, force: true });
            } catch (error) {
                // Ignore cleanup errors
            }
        }
    }

    private async getExecuteCommand(): Promise<{ command: string; args: string[] }> {
        if (this.options.execCommand) {
            const parts = this.options.execCommand.split(' ');
            return { command: parts[0], args: [...parts.slice(1)] };
        }

        // Compile TypeScript file
        const compiled = this.compileTypeScript(this.options.file);
        const tempFile = this.createTempFile(compiled, this.options.file);

        const args = [...this.options.nodeArgs, tempFile];
        
        if (this.options.inspect) {
            args.unshift('--inspect');
        }
        
        if (this.options.inspectBrk) {
            args.unshift('--inspect-brk');
        }

        // Enable source map support
        if (this.options.sourceMaps) {
            args.unshift('--enable-source-maps');
        }

        return { command: 'node', args };
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
        
        try {
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
                    FORCE_COLOR: this.options.color ? '1' : '0',
                    TS_NODE_DEV: '1',
                    NEEX_DEV: '1'
                },
                detached: true
            });

            this.startTime = new Date();
            this.restartCount++;

            if (!this.options.quiet) {
                const timestamp = new Date().toLocaleTimeString();
                loggerManager.printLine(
                    `${chalk.green(figures.play)} Started ${chalk.cyan(path.relative(process.cwd(), this.options.file))} ${chalk.dim(`(${timestamp})`)}`,
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

        } catch (error) {
            loggerManager.printLine(`Failed to start process: ${(error as Error).message}`, 'error');
            throw error;
        }
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
                this.cleanupTempFiles();
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
                    }, 3000);
                }
            } catch (error) {
                // Process might already be dead
                cleanup();
            }
        });
    }

    private invalidateCache(filePath: string): void {
        const absolutePath = path.resolve(filePath);
        
        // Remove from cache
        this.moduleCache.delete(absolutePath);
        
        // Remove dependent modules from cache
        for (const [cachedPath, module] of this.moduleCache.entries()) {
            if (module.dependencies.has(absolutePath)) {
                this.moduleCache.delete(cachedPath);
            }
        }
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

        // Clear module cache
        this.moduleCache.clear();

        await this.stopProcess();
        
        // Small delay to ensure cleanup
        await new Promise(resolve => setTimeout(resolve, 100));
        
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
            '**/.neex-temp/**',
            '**/*.log',
            '**/*.d.ts',
            ...this.options.ignore.map(pattern => `**/${pattern}/**`)
        ];

        this.watcher = watch(watchPatterns, {
            ignored,
            ignoreInitial: true,
            followSymlinks: false,
            usePolling: false,
            atomic: 200,        // سریع‌تر تشخیص تغییرات
            awaitWriteFinish: { // منتظر تمام شدن نوشتن فایل
                stabilityThreshold: 100,
                pollInterval: 50
            }
        });

        this.watcher.on('change', (filePath: string) => {
            this.invalidateCache(filePath);
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
            this.invalidateCache(filePath);
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

        // Validate TypeScript file
        const ext = path.extname(this.options.file);
        if (!['.ts', '.tsx', '.js', '.jsx'].includes(ext)) {
            throw new Error(`Unsupported file extension: ${ext}`);
        }

        loggerManager.printLine(`${chalk.blue(figures.info)} Starting TypeScript development server...`, 'info');
        
        // Show configuration in verbose mode
        if (this.options.verbose) {
            loggerManager.printLine(`Target file: ${this.options.file}`, 'info');
            loggerManager.printLine(`Watch patterns: ${this.options.watch.join(', ')}`, 'info');
            loggerManager.printLine(`Restart delay: ${this.options.delay}ms`, 'info');
            loggerManager.printLine(`Transpile only: ${this.options.transpileOnly}`, 'info');
            loggerManager.printLine(`Source maps: ${this.options.sourceMaps}`, 'info');
        }

        this.setupWatcher();
        await this.startProcess();

        loggerManager.printLine(
            `${chalk.green(figures.tick)} TypeScript development server started. Watching for changes...`,
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