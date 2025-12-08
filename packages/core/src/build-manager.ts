// src/build-manager.ts - Build manager for TypeScript projects using tsc
import { spawn, ChildProcess } from 'child_process';
import { watch } from 'chokidar';
import { loggerManager } from './logger-manager.js';
import chalk from 'chalk';
import figures from 'figures';
import path from 'path';
import fs from 'fs/promises';
import { existsSync } from 'fs';

export interface BuildOptions {
  source: string;
  output: string;
  watch: boolean;
  clean: boolean;
  minify: boolean;
  sourcemap: boolean;
  target: string;
  format: string;
  bundle: boolean;
  external: string[];
  tsconfig: string;
  verbose: boolean;
  quiet: boolean;
  color: boolean;
  analyze: boolean;
}

export class BuildManager {
  private options: BuildOptions;
  private watcher: any = null;
  private buildProcess: ChildProcess | null = null;
  private isBuilding = false;
  private buildCount = 0;

  constructor(options: BuildOptions) {
    this.options = options;
  }

  private async cleanOutputDirectory(): Promise<void> {
    if (this.options.clean && existsSync(this.options.output)) {
      try {
        await fs.rm(this.options.output, { recursive: true, force: true });
        if (this.options.verbose) {
          loggerManager.printLine(
            `Cleaned output directory: ${this.options.output}`,
            'info'
          );
        }
      } catch (error) {
        loggerManager.printLine(
          `Failed to clean output directory: ${(error as Error).message}`,
          'warn'
        );
      }
    }
  }

  private async ensureOutputDirectory(): Promise<void> {
    try {
      await fs.mkdir(this.options.output, { recursive: true });
    } catch (error) {
      throw new Error(
        `Failed to create output directory: ${(error as Error).message}`
      );
    }
  }

  private async validateTsConfig(): Promise<void> {
    if (!existsSync(this.options.tsconfig)) {
      throw new Error(
        `TypeScript config file not found: ${this.options.tsconfig}`
      );
    }
  }

  private async copyPackageJson(): Promise<void> {
    const packageJsonPath = path.join(process.cwd(), 'package.json');
    const outputPackageJsonPath = path.join(
      this.options.output,
      'package.json'
    );

    if (existsSync(packageJsonPath)) {
      try {
        const packageJson = JSON.parse(
          await fs.readFile(packageJsonPath, 'utf8')
        );

        // Create production package.json
        const prodPackageJson = {
          name: packageJson.name,
          version: packageJson.version,
          description: packageJson.description,
          main: packageJson.main?.replace(/^src\//, '') || 'index.js',
          type: this.options.format === 'esm' ? 'module' : 'commonjs',
          scripts: {
            start: 'node index.js',
          },
          dependencies: packageJson.dependencies || {},
          engines: packageJson.engines,
        };

        await fs.writeFile(
          outputPackageJsonPath,
          JSON.stringify(prodPackageJson, null, 2)
        );

        if (this.options.verbose) {
          loggerManager.printLine('Generated production package.json', 'info');
        }
      } catch (error) {
        loggerManager.printLine(
          `Failed to copy package.json: ${(error as Error).message}`,
          'warn'
        );
      }
    }
  }

  private getTscCommand(): { command: string; args: string[] } {
    const args = [
      '--project',
      this.options.tsconfig,
      '--outDir',
      this.options.output,
      '--target',
      this.options.target,
      '--declaration',
    ];

    if (this.options.sourcemap) {
      args.push('--sourceMap');
    }

    if (this.options.format === 'esm') {
      args.push('--module', 'es2020', '--moduleResolution', 'node');
    } else {
      args.push('--module', 'commonjs');
    }

    // Always include these for better compatibility
    args.push(
      '--esModuleInterop',
      '--allowSyntheticDefaultImports',
      '--strict'
    );

    return { command: 'tsc', args };
  }

  private async runBuild(): Promise<void> {
    if (this.isBuilding) {
      return;
    }

    this.isBuilding = true;
    this.buildCount++;

    const startTime = Date.now();

    if (!this.options.quiet) {
      const buildNumber = this.options.watch ? ` #${this.buildCount}` : '';
      loggerManager.printLine(
        `${chalk.green(figures.play)} Building${buildNumber}...`,
        'info'
      );
    }

    try {
      await this.ensureOutputDirectory();

      const { command, args } = this.getTscCommand();

      if (this.options.verbose) {
        loggerManager.printLine(
          `Executing: ${command} ${args.join(' ')}`,
          'info'
        );
      }

      return new Promise<void>((resolve, reject) => {
        this.buildProcess = spawn(command, args, {
          stdio: ['ignore', 'pipe', 'pipe'], // Capture stdout and stderr
          shell: false,
          env: {
            ...process.env,
            FORCE_COLOR: '0', // Disable TSC colors to avoid log pollution
          },
        });

        let stdout = '';
        let stderr = '';

        // Capture all output but don't display TSC logs
        this.buildProcess.stdout?.on('data', data => {
          stdout += data.toString();
        });

        this.buildProcess.stderr?.on('data', data => {
          stderr += data.toString();
        });

        this.buildProcess.on('error', error => {
          this.buildProcess = null;
          this.isBuilding = false;
          reject(new Error(`Build process error: ${error.message}`));
        });

        this.buildProcess.on('exit', async code => {
          this.buildProcess = null;
          this.isBuilding = false;

          const duration = Date.now() - startTime;

          if (code === 0) {
            // Copy package.json after successful build
            await this.copyPackageJson();

            if (!this.options.quiet) {
              loggerManager.printLine(
                `${chalk.green(figures.tick)} Build completed in ${duration}ms`,
                'info'
              );
            }

            if (this.options.analyze) {
              await this.analyzeBuild();
            }

            resolve();
          } else {
            // Only show meaningful errors, filter out TSC verbosity
            const meaningfulErrors = this.filterTscErrors(stderr);
            if (meaningfulErrors) {
              loggerManager.printLine(
                `Build failed:\n${meaningfulErrors}`,
                'error'
              );
            } else {
              loggerManager.printLine(
                `Build failed with code ${code}`,
                'error'
              );
            }
            reject(new Error(`Build failed with code ${code}`));
          }
        });
      });
    } catch (error) {
      this.isBuilding = false;
      throw error;
    }
  }

  private filterTscErrors(stderr: string): string {
    if (!stderr) return '';

    const lines = stderr.split('\n');
    const meaningfulLines = lines.filter(line => {
      const trimmed = line.trim();
      // Filter out TSC verbose output, keep only actual errors
      return (
        trimmed &&
        !trimmed.includes('message TS') &&
        !trimmed.includes('Found 0 errors') &&
        !trimmed.match(/^\s*\d+\s*$/) && // Filter line numbers
        !trimmed.includes('Watching for file changes')
      );
    });

    return meaningfulLines.join('\n').trim();
  }

  private async analyzeBuild(): Promise<void> {
    try {
      const files = await fs.readdir(this.options.output, {
        withFileTypes: true,
      });

      let totalSize = 0;
      const fileStats: { name: string; size: number }[] = [];

      for (const file of files) {
        if (
          file.isFile() &&
          (file.name.endsWith('.js') || file.name.endsWith('.d.ts'))
        ) {
          const filePath = path.join(this.options.output, file.name);
          const stat = await fs.stat(filePath);
          totalSize += stat.size;
          fileStats.push({ name: file.name, size: stat.size });
        }
      }

      fileStats.sort((a, b) => b.size - a.size);

      loggerManager.printLine(
        `${chalk.blue(figures.info)} Build Analysis:`,
        'info'
      );
      loggerManager.printLine(
        `Total size: ${chalk.cyan(this.formatBytes(totalSize))}`,
        'info'
      );
      loggerManager.printLine(`Generated files: ${fileStats.length}`, 'info');

      if (this.options.verbose && fileStats.length > 0) {
        const topFiles = fileStats.slice(0, 5);
        loggerManager.printLine('Largest files:', 'info');
        topFiles.forEach(file => {
          loggerManager.printLine(
            `  ${file.name}: ${this.formatBytes(file.size)}`,
            'info'
          );
        });
      }
    } catch (error) {
      loggerManager.printLine(
        `Failed to analyze build: ${(error as Error).message}`,
        'warn'
      );
    }
  }

  private async stopProcess(): Promise<void> {
    if (!this.buildProcess) {
      return;
    }

    return new Promise<void>(resolve => {
      if (!this.buildProcess) {
        resolve();
        return;
      }

      const proc = this.buildProcess;
      this.buildProcess = null;

      const cleanup = () => {
        if (!this.options.quiet) {
          loggerManager.printLine(
            `${chalk.yellow(figures.square)} Build process stopped`,
            'info'
          );
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
          }, 3000);
        }
      } catch (error) {
        // Process might already be dead
        cleanup();
      }
    });
  }

  private formatBytes(bytes: number): string {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  }

  private setupWatcher(): void {
    const watchPatterns = [
      `${this.options.source}/**/*.ts`,
      `${this.options.source}/**/*.tsx`,
      `${this.options.source}/**/*.js`,
      `${this.options.source}/**/*.jsx`,
      this.options.tsconfig,
    ];

    this.watcher = watch(watchPatterns, {
      ignoreInitial: true,
      followSymlinks: false,
      usePolling: false,
      atomic: 200,
      ignored: [
        '**/node_modules/**',
        '**/.git/**',
        `**/${this.options.output}/**`,
        '**/*.log',
        '**/*.map',
      ],
    });

    this.watcher.on('change', (filePath: string) => {
      if (this.options.verbose) {
        loggerManager.printLine(
          `File changed: ${path.relative(process.cwd(), filePath)}`,
          'info'
        );
      }
      this.debouncedBuild();
    });

    this.watcher.on('add', (filePath: string) => {
      if (this.options.verbose) {
        loggerManager.printLine(
          `File added: ${path.relative(process.cwd(), filePath)}`,
          'info'
        );
      }
      this.debouncedBuild();
    });

    this.watcher.on('unlink', (filePath: string) => {
      if (this.options.verbose) {
        loggerManager.printLine(
          `File removed: ${path.relative(process.cwd(), filePath)}`,
          'info'
        );
      }
      this.debouncedBuild();
    });

    this.watcher.on('error', (error: Error) => {
      loggerManager.printLine(`Watcher error: ${error.message}`, 'error');
    });

    if (this.options.verbose) {
      loggerManager.printLine(`Watching: ${watchPatterns.join(', ')}`, 'info');
    }
  }

  private debouncedBuild = this.debounce(this.runBuild.bind(this), 300);

  private debounce(func: Function, wait: number) {
    let timeout: NodeJS.Timeout;
    return function executedFunction(...args: any[]) {
      const later = () => {
        clearTimeout(timeout);
        func(...args);
      };
      clearTimeout(timeout);
      timeout = setTimeout(later, wait);
    };
  }

  public async build(): Promise<void> {
    // Check if source directory exists
    if (!existsSync(this.options.source)) {
      throw new Error(`Source directory not found: ${this.options.source}`);
    }

    try {
      await this.validateTsConfig();
      await this.cleanOutputDirectory();
      await this.runBuild();

      if (this.options.watch) {
        this.setupWatcher();
        if (!this.options.quiet) {
          loggerManager.printLine(
            `${chalk.blue(figures.info)} Watching for changes...`,
            'info'
          );
        }
      }
    } catch (error) {
      loggerManager.printLine((error as Error).message, 'error');
      if (!this.options.watch) {
        process.exit(1);
      }
    }
  }

  public async stop(): Promise<void> {
    if (!this.options.quiet) {
      loggerManager.printLine(
        `${chalk.yellow('⏹')} Stopping build process...`,
        'info'
      );
    }

    if (this.watcher) {
      await this.watcher.close();
      this.watcher = null;
    }

    await this.stopProcess();

    if (this.buildCount > 0 && !this.options.quiet) {
      loggerManager.printLine(
        `${chalk.blue('ℹ')} Build process stopped after ${this.buildCount} build(s).`,
        'info'
      );
    }
  }
}
