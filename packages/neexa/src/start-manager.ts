// src/start-manager.ts - Fixed production start manager
import { fork, ChildProcess } from 'child_process';
import { watch } from 'chokidar';
import { loggerManager } from './logger-manager.js';
import chalk from 'chalk';
import figures from 'figures';
import path from 'path';
import fs from 'fs';
import http from 'http';
import { debounce } from 'lodash';
import cluster from 'cluster';
import os from 'os';

export interface StartOptions {
  file: string;
  workingDir: string;
  envFile: string;
  port?: number;
  workers: number;
  memoryLimit?: string;
  logLevel: string;
  color: boolean;
  verbose: boolean;
  watch: boolean;
  maxMemory?: string;
  maxCrashes: number;
  restartDelay: number;
  healthCheck: boolean;
  healthPort: number;
  gracefulTimeout: number;
  inspect: boolean;
  inspectBrk: boolean;
  nodeArgs?: string;
}

interface WorkerInfo {
  process: ChildProcess;
  pid: number;
  restarts: number;
  startTime: Date;
  id: number;
  port: number;
}

export class StartManager {
  private options: StartOptions;
  private workers: Map<number, WorkerInfo> = new Map();
  private watcher: any = null;
  private healthServer: http.Server | null = null;
  private isShuttingDown = false;
  private debouncedRestart: () => void;
  private startTime: Date;
  private totalRestarts = 0;
  private envLoaded = false;
  private masterProcess: ChildProcess | null = null;

  constructor(options: StartOptions) {
    this.options = options;
    this.startTime = new Date();
    this.debouncedRestart = debounce(
      this.restartAll.bind(this),
      options.restartDelay
    );
  }

  private log(
    message: string,
    level: 'info' | 'warn' | 'error' = 'info'
  ): void {
    loggerManager.printLine(message, level);
  }

  private loadEnvFile(): void {
    if (this.envLoaded) return;

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
              const value = values.join('=').trim();
              const cleanValue = value.replace(/^["']|["']$/g, '');
              if (!process.env[key.trim()]) {
                process.env[key.trim()] = cleanValue;
                loadedCount++;
              }
            }
          }
        }

        if (this.options.verbose && loadedCount > 0) {
          this.log(
            `Loaded ${loadedCount} environment variables from ${this.options.envFile}`
          );
        }

        this.envLoaded = true;
      } catch (error) {
        if (this.options.verbose) {
          this.log(
            `Failed to load environment file: ${(error as Error).message}`,
            'warn'
          );
        }
      }
    }
  }

  private parseMemoryLimit(limit: string): number | undefined {
    if (!limit) return undefined;

    const match = limit.match(/^(\d+)([KMGT]?)$/i);
    if (!match) return undefined;

    const value = parseInt(match[1]);
    const unit = match[2]?.toUpperCase() || '';

    const multipliers = {
      K: 1024,
      M: 1024 * 1024,
      G: 1024 * 1024 * 1024,
      T: 1024 * 1024 * 1024 * 1024,
    };
    return value * (multipliers[unit as keyof typeof multipliers] || 1);
  }

  private getNodeArgs(): string[] {
    const args: string[] = [];

    if (this.options.memoryLimit) {
      const memoryBytes = this.parseMemoryLimit(this.options.memoryLimit);
      if (memoryBytes) {
        const memoryMB = Math.floor(memoryBytes / (1024 * 1024));
        args.push(`--max-old-space-size=${memoryMB}`);
      }
    }

    if (this.options.inspect) {
      args.push('--inspect');
    }

    if (this.options.inspectBrk) {
      args.push('--inspect-brk');
    }

    if (this.options.nodeArgs) {
      args.push(...this.options.nodeArgs.split(' ').filter(arg => arg.trim()));
    }

    return args;
  }

  private async startSingleProcess(): Promise<void> {
    const nodeArgs = this.getNodeArgs();
    const port = this.options.port || 8000;

    const env: { [key: string]: string | undefined } = {
      ...process.env,
      NODE_ENV: process.env.NODE_ENV || 'production',
      PORT: port.toString(),
      FORCE_COLOR: this.options.color ? '1' : '0',
      NODE_OPTIONS: '--no-deprecation',
    };

    this.masterProcess = fork(this.options.file, [], {
      cwd: this.options.workingDir,
      env,
      execArgv: nodeArgs,
      silent: false, // Let the process handle its own output
    });

    this.masterProcess.on('error', error => {
      this.log(`Process error: ${error.message}`, 'error');
    });

    this.masterProcess.on('exit', (code, signal) => {
      if (!this.isShuttingDown && code !== 0 && signal !== 'SIGTERM') {
        this.log(`Process crashed (code: ${code}, signal: ${signal})`, 'error');
        this.totalRestarts++;

        if (this.totalRestarts < this.options.maxCrashes) {
          setTimeout(() => {
            this.startSingleProcess();
          }, this.options.restartDelay);
        } else {
          this.log(
            `Max crashes reached (${this.options.maxCrashes}), not restarting`,
            'error'
          );
        }
      }
    });

    // Wait for process to be ready
    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        resolve(); // Don't reject, assume it's ready
      }, 5000);

      this.masterProcess!.on('message', (message: any) => {
        if (message && message.type === 'ready') {
          clearTimeout(timeout);
          resolve();
        }
      });

      this.masterProcess!.on('error', error => {
        clearTimeout(timeout);
        reject(error);
      });

      this.masterProcess!.on('exit', code => {
        clearTimeout(timeout);
        if (code !== 0) {
          reject(new Error(`Process exited with code ${code}`));
        } else {
          resolve();
        }
      });
    });

    if (this.options.verbose) {
      this.log(
        `Process started (PID: ${this.masterProcess.pid}, Port: ${port})`
      );
    }
  }

  private startWorker(workerId: number): Promise<WorkerInfo> {
    return new Promise((resolve, reject) => {
      const nodeArgs = this.getNodeArgs();
      const port = this.options.port || 8000;

      const env: { [key: string]: string | undefined } = {
        ...process.env,
        NODE_ENV: process.env.NODE_ENV || 'production',
        WORKER_ID: workerId.toString(),
        PORT: port.toString(),
        CLUSTER_WORKER: 'true',
        FORCE_COLOR: this.options.color ? '1' : '0',
        NODE_OPTIONS: '--no-deprecation',
      };

      const workerProcess = fork(this.options.file, [], {
        cwd: this.options.workingDir,
        env,
        execArgv: nodeArgs,
        silent: true,
      });

      const workerInfo: WorkerInfo = {
        process: workerProcess,
        pid: workerProcess.pid!,
        restarts: 0,
        startTime: new Date(),
        id: workerId,
        port: port,
      };

      this.workers.set(workerId, workerInfo);

      let isReady = false;

      const readinessTimeout = setTimeout(() => {
        if (!isReady) {
          workerProcess.kill();
          reject(
            new Error(`Worker ${workerId} failed to become ready within 15s.`)
          );
        }
      }, 15000);

      const cleanupReadinessListeners = () => {
        clearTimeout(readinessTimeout);
        workerProcess.stdout?.removeListener('data', onDataForReady);
        workerProcess.removeListener('message', onMessageForReady);
      };

      const onReady = () => {
        if (isReady) return;
        isReady = true;
        cleanupReadinessListeners();
        if (this.options.verbose) {
          this.log(`Worker ${workerId} is ready (PID: ${workerProcess.pid})`);
        }
        resolve(workerInfo);
      };

      const onDataForReady = (data: Buffer) => {
        const message = data.toString();
        const prefix = chalk.dim(`[Worker ${workerId}] `);
        process.stdout.write(prefix + message);

        if (/listening|ready|running on port|local:/i.test(message)) {
          onReady();
        }
      };

      const onMessageForReady = (message: any) => {
        if (message && message.type === 'ready') {
          onReady();
        }
      };

      workerProcess.stdout?.on('data', onDataForReady);
      workerProcess.stderr?.on('data', data => {
        const prefix = chalk.red.dim(`[Worker ${workerId}] `);
        process.stderr.write(prefix + data.toString());
      });
      workerProcess.on('message', onMessageForReady);

      workerProcess.on('error', error => {
        if (!isReady) {
          cleanupReadinessListeners();
          reject(error);
        }
        this.log(`Worker ${workerId} error: ${error.message}`, 'error');
      });

      workerProcess.on('exit', (code, signal) => {
        if (!isReady) {
          cleanupReadinessListeners();
          reject(
            new Error(
              `Worker ${workerId} exited with code ${code} before becoming ready.`
            )
          );
        } else {
          this.workers.delete(workerId);
          if (!this.isShuttingDown && code !== 0 && signal !== 'SIGTERM') {
            this.log(
              `Worker ${workerId} crashed (code: ${code}, signal: ${signal})`,
              'error'
            );
            this.restartWorker(workerId);
          }
        }
      });
    });
  }

  private async restartWorker(workerId: number): Promise<void> {
    const workerInfo = this.workers.get(workerId);
    if (workerInfo) {
      workerInfo.restarts++;
      this.totalRestarts++;

      if (workerInfo.restarts >= this.options.maxCrashes) {
        this.log(
          `Worker ${workerId} reached max crashes (${this.options.maxCrashes}), not restarting`,
          'error'
        );
        return;
      }

      if (this.options.verbose) {
        this.log(
          `Restarting worker ${workerId} (attempt ${workerInfo.restarts})`
        );
      }

      try {
        workerInfo.process.kill('SIGTERM');
        await this.waitForProcessExit(workerInfo.process, 5000);
      } catch (error) {
        workerInfo.process.kill('SIGKILL');
      }

      setTimeout(() => {
        this.startWorker(workerId);
      }, this.options.restartDelay);
    }
  }

  private async waitForProcessExit(
    process: ChildProcess,
    timeout: number
  ): Promise<void> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error('Process exit timeout'));
      }, timeout);

      process.on('exit', () => {
        clearTimeout(timer);
        resolve();
      });

      if (process.killed) {
        clearTimeout(timer);
        resolve();
      }
    });
  }

  private async startCluster(): Promise<void> {
    if (this.options.workers === 1) {
      // Single process mode
      await this.startSingleProcess();
      return;
    }

    // Multi-worker mode
    this.log(
      `${chalk.blue(figures.play)} Starting production server (${this.options.workers} workers)`
    );

    const startPromises: Promise<WorkerInfo>[] = [];

    for (let i = 0; i < this.options.workers; i++) {
      startPromises.push(this.startWorker(i + 1));
    }

    try {
      await Promise.all(startPromises);
      this.log(
        `${chalk.green(figures.tick)} Server ready on port ${this.options.port || 8000} (${this.workers.size} workers)`
      );
    } catch (error) {
      this.log(
        `Failed to start some workers: ${(error as Error).message}`,
        'error'
      );
      if (this.workers.size > 0) {
        this.log(
          `${chalk.yellow(figures.warning)} Server partially ready on port ${this.options.port || 8000} (${this.workers.size} workers)`
        );
      }
    }
  }

  private setupHealthCheck(): void {
    if (!this.options.healthCheck) return;

    this.healthServer = http.createServer((req, res) => {
      res.setHeader('Access-Control-Allow-Origin', '*');
      res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
      res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

      if (req.method === 'OPTIONS') {
        res.writeHead(200);
        res.end();
        return;
      }

      if (req.url === '/health') {
        const stats = {
          status: 'ok',
          uptime: Date.now() - this.startTime.getTime(),
          workers: this.workers.size,
          activeWorkers: Array.from(this.workers.values()).map(w => ({
            id: w.id,
            pid: w.pid,
            restarts: w.restarts,
            uptime: Date.now() - w.startTime.getTime(),
          })),
          totalRestarts: this.totalRestarts,
          memory: process.memoryUsage(),
          cpu: os.loadavg(),
          port: this.options.port || 8000,
        };

        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(stats, null, 2));
      } else {
        res.writeHead(404, { 'Content-Type': 'text/plain' });
        res.end('Not Found');
      }
    });

    this.healthServer.listen(this.options.healthPort, () => {
      if (this.options.verbose) {
        this.log(
          `Health endpoint: http://localhost:${this.options.healthPort}/health`
        );
      }
    });
  }

  private setupWatcher(): void {
    if (!this.options.watch) return;

    const watchPatterns = [
      `${this.options.workingDir}/**/*.js`,
      `${this.options.workingDir}/**/*.json`,
      `${this.options.workingDir}/**/*.env*`,
    ];

    this.watcher = watch(watchPatterns, {
      ignored: ['**/node_modules/**', '**/.git/**', '**/logs/**', '**/tmp/**'],
      ignoreInitial: true,
      atomic: 300,
      awaitWriteFinish: {
        stabilityThreshold: 2000,
        pollInterval: 100,
      },
    });

    this.watcher.on('change', (filePath: string) => {
      if (this.options.verbose) {
        this.log(
          `File changed: ${path.relative(this.options.workingDir, filePath)}`
        );
      }
      this.debouncedRestart();
    });

    this.watcher.on('error', (error: Error) => {
      this.log(`Watcher error: ${error.message}`, 'error');
    });

    if (this.options.verbose) {
      this.log('File watching enabled');
    }
  }

  private async restartAll(): Promise<void> {
    if (this.isShuttingDown) return;

    this.log('Restarting due to file changes...');

    if (this.options.workers === 1 && this.masterProcess) {
      // Single process restart
      try {
        this.masterProcess.kill('SIGTERM');
        await this.waitForProcessExit(this.masterProcess, 5000);
      } catch (error) {
        this.masterProcess.kill('SIGKILL');
      }

      setTimeout(() => {
        this.startSingleProcess();
      }, this.options.restartDelay);
    } else {
      // Multi-worker restart
      const restartPromises: Promise<void>[] = [];

      for (const [workerId, workerInfo] of this.workers.entries()) {
        restartPromises.push(
          (async () => {
            try {
              workerInfo.process.kill('SIGTERM');
              await this.waitForProcessExit(workerInfo.process, 5000);
            } catch (error) {
              workerInfo.process.kill('SIGKILL');
            }
          })()
        );
      }

      await Promise.allSettled(restartPromises);

      setTimeout(() => {
        this.startCluster();
      }, this.options.restartDelay);
    }
  }

  public async start(): Promise<void> {
    try {
      // Load environment variables
      this.loadEnvFile();

      // Set up monitoring and health checks
      this.setupHealthCheck();
      this.setupWatcher();

      // Start the application
      await this.startCluster();
    } catch (error) {
      this.log(`Failed to start server: ${(error as Error).message}`, 'error');
      throw error;
    }
  }

  public async stop(): Promise<void> {
    if (this.isShuttingDown) return;

    this.isShuttingDown = true;
    this.log(`${chalk.yellow('⏹')} Shutting down gracefully...`);

    if (this.watcher) {
      await this.watcher.close();
    }

    if (this.healthServer) {
      this.healthServer.close();
    }

    // Stop single process
    if (this.masterProcess) {
      try {
        this.masterProcess.kill('SIGTERM');
        await this.waitForProcessExit(
          this.masterProcess,
          this.options.gracefulTimeout
        );
      } catch (error) {
        this.masterProcess.kill('SIGKILL');
      }
    }

    // Stop workers
    const shutdownPromises: Promise<void>[] = [];
    for (const [workerId, workerInfo] of this.workers.entries()) {
      shutdownPromises.push(
        (async () => {
          try {
            workerInfo.process.kill('SIGTERM');
            await this.waitForProcessExit(
              workerInfo.process,
              this.options.gracefulTimeout
            );
          } catch (error) {
            workerInfo.process.kill('SIGKILL');
          }
        })()
      );
    }

    await Promise.allSettled(shutdownPromises);

    this.workers.clear();
  }
}
