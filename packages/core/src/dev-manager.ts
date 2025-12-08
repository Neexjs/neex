// src/dev-manager.ts - Ultra-fast TypeScript development server like tsx
import { spawn, ChildProcess } from 'child_process';
import { watch } from 'chokidar';
import { loggerManager } from './logger-manager.js';
import chalk from 'chalk';
import figures from 'figures';
import path from 'path';
import fs from 'fs';
import { debounce } from 'lodash';
import * as ts from 'typescript';
import crypto from 'crypto';

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

interface ModuleInfo {
  code: string;
  map?: string;
  hash: string;
  timestamp: number;
  dependencies: string[];
}

export class DevManager {
  private options: DevOptions;
  private process: ChildProcess | null = null;
  private watcher: any = null;
  private isRestarting = false;
  private restartCount = 0;
  private startTime: Date | null = null;
  private debouncedRestart: () => void;
  private moduleCache = new Map<string, ModuleInfo>();
  private tsCompilerOptions: ts.CompilerOptions;
  private tempDir: string;
  private currentTempFile: string | null = null;
  private isShuttingDown = false;

  constructor(options: DevOptions) {
    this.options = options;
    this.tempDir = path.join(process.cwd(), '.neex-temp');
    this.debouncedRestart = debounce(
      this.restart.bind(this),
      Math.max(options.delay, 100)
    );
    this.tsCompilerOptions = this.loadTsConfig();
    this.setupTempDir();
  }

  private setupTempDir(): void {
    if (fs.existsSync(this.tempDir)) {
      try {
        fs.rmSync(this.tempDir, { recursive: true, force: true });
      } catch (error) {
        // Ignore cleanup errors
      }
    }
    fs.mkdirSync(this.tempDir, { recursive: true });
  }

  private loadTsConfig(): ts.CompilerOptions {
    const configPath = this.options.tsConfig || 'tsconfig.json';
    const defaultOptions: ts.CompilerOptions = {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.CommonJS,
      moduleResolution: ts.ModuleResolutionKind.NodeJs,
      allowJs: true,
      strict: false,
      esModuleInterop: true,
      skipLibCheck: true,
      forceConsistentCasingInFileNames: true,
      resolveJsonModule: true,
      declaration: false,
      sourceMap: this.options.sourceMaps,
      inlineSourceMap: false,
      inlineSources: false,
      removeComments: false,
      preserveConstEnums: false,
      isolatedModules: true, // For faster compilation
    };

    if (fs.existsSync(configPath)) {
      try {
        const configFile = ts.readConfigFile(configPath, ts.sys.readFile);
        if (!configFile.error) {
          const parsedConfig = ts.parseJsonConfigFileContent(
            configFile.config,
            ts.sys,
            path.dirname(configPath)
          );

          if (parsedConfig.errors.length === 0) {
            return { ...defaultOptions, ...parsedConfig.options };
          }
        }
      } catch (error) {
        // Fall back to defaults
      }
    }

    return defaultOptions;
  }

  private loadEnvFile(): void {
    let count = 0;
    const envFile = this.options.envFile;

    if (envFile && fs.existsSync(envFile)) {
      try {
        const envContent = fs.readFileSync(envFile, 'utf8');
        const lines = envContent.split('\n');

        for (const line of lines) {
          const trimmed = line.trim();
          if (trimmed && !trimmed.startsWith('#')) {
            const [key, ...values] = trimmed.split('=');
            if (key && values.length > 0) {
              process.env[key.trim()] = values
                .join('=')
                .trim()
                .replace(/^["']|["']$/g, '');
              count++;
            }
          }
        }

        if (!this.options.quiet) {
          loggerManager.printLine(
            `${chalk.green(figures.play)} Loaded ${count} env variables from ${envFile}`,
            'info'
          );
        }
      } catch (error) {
        if (this.options.verbose) {
          loggerManager.printLine(
            `Failed to load ${envFile}: ${(error as Error).message}`,
            'warn'
          );
        }
      }
    }
  }

  private createHash(content: string): string {
    return crypto.createHash('md5').update(content).digest('hex');
  }

  private extractDependencies(sourceCode: string, filePath: string): string[] {
    const dependencies: string[] = [];
    const importRegex =
      /(?:import|require)\s*(?:\([^)]*\)|[^;]+?from\s+)?['"`]([^'"`]+)['"`]/g;
    let match;

    while ((match = importRegex.exec(sourceCode)) !== null) {
      const importPath = match[1];
      if (importPath.startsWith('.')) {
        let resolvedPath = path.resolve(path.dirname(filePath), importPath);

        // Try to resolve with extensions
        if (!fs.existsSync(resolvedPath)) {
          for (const ext of ['.ts', '.tsx', '.js', '.jsx']) {
            const withExt = resolvedPath + ext;
            if (fs.existsSync(withExt)) {
              resolvedPath = withExt;
              break;
            }
          }
        }

        // Try index files
        if (!fs.existsSync(resolvedPath)) {
          for (const ext of ['.ts', '.tsx', '.js', '.jsx']) {
            const indexPath = path.join(resolvedPath, 'index' + ext);
            if (fs.existsSync(indexPath)) {
              resolvedPath = indexPath;
              break;
            }
          }
        }

        if (fs.existsSync(resolvedPath)) {
          dependencies.push(resolvedPath);
        }
      }
    }

    return dependencies;
  }

  private compileModule(
    filePath: string,
    forceRecompile: boolean = false
  ): ModuleInfo {
    const absolutePath = path.resolve(filePath);

    try {
      const sourceCode = fs.readFileSync(absolutePath, 'utf8');
      const hash = this.createHash(sourceCode);
      const cached = this.moduleCache.get(absolutePath);

      // Check if we can use cached version
      if (!forceRecompile && cached && cached.hash === hash) {
        return cached;
      }

      const dependencies = this.extractDependencies(sourceCode, absolutePath);

      // Fast transpile without type checking for development
      const result = ts.transpileModule(sourceCode, {
        compilerOptions: this.tsCompilerOptions,
        fileName: absolutePath,
        reportDiagnostics: false, // Skip diagnostics for speed
      });

      const moduleInfo: ModuleInfo = {
        code: result.outputText,
        map: result.sourceMapText,
        hash,
        timestamp: Date.now(),
        dependencies,
      };

      this.moduleCache.set(absolutePath, moduleInfo);

      if (this.options.verbose) {
        loggerManager.printLine(
          `Compiled ${path.relative(process.cwd(), filePath)}`,
          'info'
        );
      }

      return moduleInfo;
    } catch (error) {
      loggerManager.printLine(
        `Compilation error: ${(error as Error).message}`,
        'error'
      );
      throw error;
    }
  }

  private invalidateModuleCache(filePath: string): void {
    const absolutePath = path.resolve(filePath);

    // Remove the file itself
    this.moduleCache.delete(absolutePath);

    // Remove any modules that depend on this file
    const toRemove: string[] = [];
    for (const [cachedPath, info] of this.moduleCache.entries()) {
      if (info.dependencies.includes(absolutePath)) {
        toRemove.push(cachedPath);
      }
    }

    for (const pathToRemove of toRemove) {
      this.moduleCache.delete(pathToRemove);
    }

    if (this.options.verbose && toRemove.length > 0) {
      loggerManager.printLine(
        `Invalidated ${toRemove.length + 1} modules`,
        'info'
      );
    }
  }

  private createExecutableFile(): string {
    // Always force recompile the main file
    const mainModule = this.compileModule(this.options.file, true);

    // Create a unique temp file
    const timestamp = Date.now();
    const random = Math.random().toString(36).substr(2, 9);
    const tempFile = path.join(this.tempDir, `main-${timestamp}-${random}.js`);

    let code = mainModule.code;

    // Add source map support
    if (mainModule.map && this.options.sourceMaps) {
      const mapFile = tempFile + '.map';
      fs.writeFileSync(mapFile, mainModule.map);
      code += `\n//# sourceMappingURL=${path.basename(mapFile)}`;
    }

    fs.writeFileSync(tempFile, code);

    // Clean up old temp file
    if (this.currentTempFile && fs.existsSync(this.currentTempFile)) {
      try {
        fs.unlinkSync(this.currentTempFile);
        const mapFile = this.currentTempFile + '.map';
        if (fs.existsSync(mapFile)) {
          fs.unlinkSync(mapFile);
        }
      } catch (error) {
        // Ignore cleanup errors
      }
    }

    this.currentTempFile = tempFile;
    return tempFile;
  }

  private async getExecuteCommand(): Promise<{
    command: string;
    args: string[];
  }> {
    if (this.options.execCommand) {
      const parts = this.options.execCommand.split(' ');
      return { command: parts[0], args: parts.slice(1) };
    }

    const executableFile = this.createExecutableFile();
    const args = [...this.options.nodeArgs, executableFile];

    // Add Node.js flags
    if (this.options.inspect) args.unshift('--inspect');
    if (this.options.inspectBrk) args.unshift('--inspect-brk');
    if (this.options.sourceMaps) args.unshift('--enable-source-maps');

    return { command: 'node', args };
  }

  private clearConsole(): void {
    if (this.options.clearConsole && process.stdout.isTTY) {
      process.stdout.write('\x1Bc'); // Clear screen and scrollback
    }
  }

  private async startProcess(): Promise<void> {
    if (this.process) return;

    this.loadEnvFile();

    try {
      const { command, args } = await this.getExecuteCommand();

      this.process = spawn(command, args, {
        stdio: ['ignore', 'pipe', 'pipe'],
        env: {
          ...process.env,
          NODE_ENV: process.env.NODE_ENV || 'development',
          FORCE_COLOR: this.options.color ? '1' : '0',
          NODE_OPTIONS: '--max-old-space-size=4096', // Prevent memory issues
        },
        detached: false, // Keep attached for better cleanup
      });

      this.startTime = new Date();
      this.restartCount++;

      // Handle stdout/stderr
      this.process.stdout?.on('data', data => {
        process.stdout.write(data);
      });

      this.process.stderr?.on('data', data => {
        process.stderr.write(data);
      });

      this.process.on('error', error => {
        loggerManager.printLine(`Process error: ${error.message}`, 'error');
      });

      this.process.on('exit', (code, signal) => {
        if (this.process) {
          this.process = null;

          if (!this.isRestarting && code !== 0) {
            const duration = this.startTime
              ? Date.now() - this.startTime.getTime()
              : 0;
            loggerManager.printLine(
              `${chalk.red('✖')} Process exited with code ${code} (${duration}ms)`,
              'error'
            );
          }
        }
      });
    } catch (error) {
      loggerManager.printLine(
        `Failed to start: ${(error as Error).message}`,
        'error'
      );
      throw error;
    }
  }

  private async stopProcess(): Promise<void> {
    if (!this.process) return;

    return new Promise<void>(resolve => {
      const proc = this.process!;
      this.process = null;

      const cleanup = () => {
        resolve();
      };

      proc.on('exit', cleanup);
      proc.on('error', cleanup);

      try {
        proc.kill('SIGTERM');

        // Force kill after timeout
        setTimeout(() => {
          if (!proc.killed) {
            proc.kill('SIGKILL');
          }
        }, 1000);
      } catch (error) {
        cleanup();
      }
    });
  }

  private async restart(): Promise<void> {
    if (this.isRestarting) return;

    this.isRestarting = true;

    // Clear console immediately for better UX
    this.clearConsole();

    if (!this.options.quiet) {
      loggerManager.printLine(`${chalk.yellow('⟳')} Restarting...`, 'info');
    }

    // Stop current process
    await this.stopProcess();

    // Start new process
    await this.startProcess();

    this.isRestarting = false;
  }

  private setupWatcher(): void {
    const watchPatterns = this.options.watch;

    // Optimized ignore patterns
    const ignored = [
      'node_modules/**',
      '.git/**',
      'dist/**',
      'build/**',
      '.neex-temp/**',
      '**/*.log',
      '**/*.d.ts',
      '**/*.map',
      '**/*.tsbuildinfo',
      ...this.options.ignore,
    ];

    this.watcher = watch(watchPatterns, {
      ignored,
      ignoreInitial: true,
      followSymlinks: false,
      usePolling: false,
      atomic: 50, // Very fast detection
      awaitWriteFinish: {
        stabilityThreshold: 100,
        pollInterval: 50,
      },
    });

    this.watcher.on('change', (filePath: string) => {
      this.invalidateModuleCache(filePath);

      if (this.options.verbose) {
        loggerManager.printLine(
          `${chalk.blue('●')} ${path.relative(process.cwd(), filePath)}`,
          'info'
        );
      }

      this.debouncedRestart();
    });

    this.watcher.on('add', (filePath: string) => {
      if (this.options.verbose) {
        loggerManager.printLine(
          `${chalk.green('+')} ${path.relative(process.cwd(), filePath)}`,
          'info'
        );
      }
      this.debouncedRestart();
    });

    this.watcher.on('unlink', (filePath: string) => {
      this.invalidateModuleCache(filePath);

      if (this.options.verbose) {
        loggerManager.printLine(
          `${chalk.red('-')} ${path.relative(process.cwd(), filePath)}`,
          'info'
        );
      }

      this.debouncedRestart();
    });

    this.watcher.on('error', (error: Error) => {
      loggerManager.printLine(`Watcher error: ${error.message}`, 'error');
    });
  }

  public async start(): Promise<void> {
    if (!fs.existsSync(this.options.file)) {
      throw new Error(`Target file not found: ${this.options.file}`);
    }

    const ext = path.extname(this.options.file);
    if (!['.ts', '.tsx', '.js', '.jsx'].includes(ext)) {
      throw new Error(`Unsupported file extension: ${ext}`);
    }

    // Clear any existing cache
    this.moduleCache.clear();
    this.setupTempDir();

    if (!this.options.quiet) {
      loggerManager.printLine(
        `${chalk.green(figures.play)} Starting TypeScript development server...`,
        'info'
      );
    }

    this.setupWatcher();
    await this.startProcess();

    if (!this.options.quiet) {
      loggerManager.printLine(
        `${chalk.green('✓')} Watching for changes...`,
        'info'
      );
    }
  }

  public async stop(): Promise<void> {
    if (this.isShuttingDown) return;
    this.isShuttingDown = true;

    if (!this.options.quiet) {
      loggerManager.printLine(
        `${chalk.yellow('⏹')} Stopping dev server...`,
        'info'
      );
    }

    if (this.watcher) {
      await this.watcher.close();
      this.watcher = null;
    }

    await this.stopProcess();

    // Cleanup temp files
    if (fs.existsSync(this.tempDir)) {
      try {
        fs.rmSync(this.tempDir, { recursive: true, force: true });
      } catch (error) {
        // Ignore cleanup errors
      }
    }

    this.moduleCache.clear();
  }
}
