// src/runner.ts
import path from 'path';
import chalk from 'chalk';
import logger from './logger';
import pMap from 'p-map';
import { MonorepoManager } from './monorepo';
import { ConfigLoader } from './config';
import { TaskHasher } from './TaskHasher';
import { CacheManager, CacheMeta } from './cache';
import {
  RunOptions,
  RunResult,
  NeexConfig,
  TaskConfig,
  ServerInfo,
  CommandOutput
} from './types';
import type { Subprocess } from "bun";

export class Runner {
  private options: RunOptions;
  private activeProcesses: Map<string, Subprocess> = new Map();
  private serverInfo: Map<string, ServerInfo> = new Map();
  private portRegex =
    /listening on (?:port |http:\/\/localhost:|https:\/\/localhost:)(\d+)/i;
  private urlRegex = /(https?:\/\/localhost:[0-9]+(?:\/[^\s]*)?)/i;
  private isCleaningUp = false;
  
  // Cache System
  private cacheManager?: CacheManager;
  private taskHasher?: TaskHasher;
  private monorepoManager?: MonorepoManager;
  private config?: NeexConfig;
  private rootDir: string;

  constructor(options: RunOptions, rootDir: string = process.cwd()) {
    this.options = options;
    this.rootDir = rootDir;
    this.activeProcesses = new Map();
    this.setupSignalHandlers();
  }

  private async ensureInitialized() {
      if (this.cacheManager) return;
      
      try {
          this.config = await ConfigLoader.load(this.rootDir);
          // MonorepoManager needs runner instance for graph operations if circular?
          // The error says it requires 2 arguments.
          this.monorepoManager = new MonorepoManager(this.rootDir, this); 
          await this.monorepoManager.loadConfig();
          await this.monorepoManager.scanWorkspaces();
          
          this.taskHasher = new TaskHasher(this.rootDir, this.monorepoManager, this.config);
          this.cacheManager = new CacheManager(this.rootDir);
      } catch (e) {
          logger.printLine(`Failed to initialize cache system: ${(e as Error).message}`, 'warn');
      }
  }

  private setupSignalHandlers(): void {
    const signals: NodeJS.Signals[] = ['SIGINT', 'SIGTERM', 'SIGQUIT'];

    signals.forEach(signal => {
      process.on(signal, () => {
        if (!this.isCleaningUp) {
          this.isCleaningUp = true;
          logger.printLine(`\nReceived ${signal}. Cleaning up...`, 'warn');
          this.cleanup(signal);
          process.exit(0);
        }
      });
    });

    // Handle unexpected exits
    process.on('beforeExit', () => {
      if (!this.isCleaningUp) {
        this.cleanup();
      }
    });
  }

  private async expandWildcardCommands(commands: string[]): Promise<string[]> {
    const expandedCommands: string[] = [];
    let packageJson: any;
    try {
      const packageJsonPath = path.join(this.rootDir, 'package.json');
      const file = Bun.file(packageJsonPath);
      if (await file.exists()) {
        packageJson = await file.json();
      }
    } catch (error) {
      logger.printLine(
        `Could not read or parse package.json: ${(error as Error).message}`,
        'warn'
      );
      packageJson = { scripts: {} };
    }

    for (const command of commands) {
      if (command.includes('*') && packageJson && packageJson.scripts) {
        const pattern = new RegExp(`^${command.replace(/\*/g, '.*')}$`);
        let foundMatch = false;
        for (const scriptName in packageJson.scripts) {
          if (pattern.test(scriptName)) {
            expandedCommands.push(scriptName);
            foundMatch = true;
          }
        }
        if (!foundMatch) {
          logger.printLine(
            `No scripts found in package.json matching wildcard: ${command}`,
            'warn'
          );
          expandedCommands.push(command);
        }
      } else {
        expandedCommands.push(command);
      }
    }
    return expandedCommands;
  }

  private async resolveScriptAndCwd(
    scriptNameOrCommand: string,
    baseDir: string
  ): Promise<{ executableCommand: string; executionCwd?: string; isScript: boolean }> {
    try {
      const packageJsonPath = path.join(baseDir, 'package.json');
      const packageJson = await Bun.file(packageJsonPath).json();

      if (packageJson.scripts && packageJson.scripts[scriptNameOrCommand]) {
        const scriptValue: string = packageJson.scripts[scriptNameOrCommand];

        const cdMatch = scriptValue.match(/^cd\s+([^&]+)\s+&&\s+(.*)$/);
        if (cdMatch) {
          const dir = cdMatch[1];
          const commandToExecute = cdMatch[2];
          const targetCwd = path.resolve(baseDir, dir);
          return {
            executableCommand: commandToExecute,
            executionCwd: targetCwd,
            isScript: true
          };
        } else {
          return { executableCommand: scriptValue, executionCwd: baseDir, isScript: true };
        }
      }
    } catch (error) {
      // Will treat as direct command
    }

    return { executableCommand: scriptNameOrCommand, executionCwd: undefined, isScript: false };
  }

  detectServerInfo(command: string, data: string): void {
    if (!this.options.isServerMode) return;

    let serverInfo = this.serverInfo.get(command);
    if (!serverInfo) {
      serverInfo = {
        name: command,
        status: 'starting',
      };
      this.serverInfo.set(command, serverInfo);
    }

    const portMatch = data.match(this.portRegex);
    if (portMatch && portMatch[1]) {
      serverInfo.port = parseInt(portMatch[1], 10);
      serverInfo.status = 'running';

      if (!serverInfo.url) {
        logger.printLine(
          `Server ${command} running on port ${serverInfo.port}`,
          'info'
        );
      }
    }

    const urlMatch = data.match(this.urlRegex);
    if (urlMatch && urlMatch[1]) {
      serverInfo.url = urlMatch[1];
      serverInfo.status = 'running';
      logger.printLine(
        `Server ${command} available at ${chalk.cyan(serverInfo.url)}`,
        'info'
      );
    }

    this.serverInfo.set(command, serverInfo);
  }

  async runCommand(
    originalCommand: string,
    currentRetry = 0
  ): Promise<RunResult> {
    await this.ensureInitialized();
    
    // 1. Resolve Command Context
    const { executableCommand: command, executionCwd, isScript } =
      await this.resolveScriptAndCwd(originalCommand, this.rootDir);
      
    const cwd = executionCwd || process.cwd();
    
    // 2. Attempt Caching
    let pkgName = 'unknown';
    let hash: string | null = null;
    let taskConfig: TaskConfig = { cache: true }; 

    if (this.config && this.monorepoManager && isScript) {
        try {
            const pkgInfo = await this.readPackageJson(cwd);
            pkgName = pkgInfo.name;
            taskConfig = this.config.pipeline?.[originalCommand] || { cache: false }; 
            
            if (taskConfig.cache !== false) {
                 if (this.config.pipeline && this.config.pipeline[originalCommand]) {
                     hash = await this.taskHasher!.hashTask(pkgName, originalCommand, taskConfig);
                     
                     const outputs = taskConfig.outputs || (pkgInfo.version ? ['dist'] : []); 
                     
                     if (hash) {
                         const metadata = await this.cacheManager!.restore(hash, outputs);
                         if (metadata) {
                             return {
                                 command: originalCommand,
                                 success: metadata.exitCode === 0,
                                 code: metadata.exitCode,
                                 startTime: new Date(metadata.timestamp),
                                 endTime: new Date(metadata.timestamp + metadata.duration),
                                 duration: metadata.duration,
                                 output: metadata.stdout.concat(metadata.stderr),
                                 stderr: metadata.stderr
                             };
                         }
                     }
                 } else {
                     // Task not in pipeline, skip cache
                 }
            }
        } catch (e) {
             // Cache initialization failed, continue without caching
        }
    }

    const startTime = new Date();
    const result: RunResult = {
      command: originalCommand,
      success: false,
      code: null,
      startTime,
      endTime: null,
      duration: 0,
      output: [],
      stderr: [],
    };

    if (this.options.printOutput) {
      logger.printStart(originalCommand);
    }

    return new Promise<RunResult>(async resolve => {
      const env = {
        ...process.env,
        FORCE_COLOR: this.options.color ? '1' : '0',
      };

      try {
          // Use shell execution to support scripts, builtins (exit), and correct arg parsing
          const proc = Bun.spawn(['sh', '-c', command], {
            cwd,
            env,
            stdout: 'pipe',
            stderr: 'pipe',
          });

          this.activeProcesses.set(originalCommand, proc);

          if (this.options.isServerMode) {
            this.serverInfo.set(originalCommand, {
              name: originalCommand,
              status: 'starting',
              pid: proc.pid,
              startTime: new Date(),
            });
          }

          const Decoder = new TextDecoder();
          
          const readStream = async (stream: ReadableStream, type: 'stdout' | 'stderr') => {
              // @ts-ignore
              for await (const chunk of stream) {
                  const text = Decoder.decode(chunk);
                  const output: CommandOutput = {
                      command: originalCommand,
                      type,
                      data: text,
                      timestamp: new Date(),
                  };
                  
                  if (type === 'stdout' && this.options.isServerMode)
                      this.detectServerInfo(originalCommand, text);
                      
                  if (result.output) result.output.push(output);
                  if (type === 'stderr' && result.stderr) result.stderr.push(output);
                  
                  logger.bufferOutput(output);
                  if (!this.options.groupOutput && this.options.printOutput)
                      logger.printBuffer(originalCommand);
              }
          };

          // Process streams concurrently
          await Promise.all([
             readStream(proc.stdout, 'stdout'),
             readStream(proc.stderr, 'stderr')
          ]);

          const exitCode = await proc.exited;
          
          result.code = exitCode;
          result.success = exitCode === 0;
          result.endTime = new Date();
          result.duration = result.endTime.getTime() - startTime.getTime();
          this.activeProcesses.delete(originalCommand);

          if (this.options.isServerMode) {
               // Update status if stopped?
               // ...
          }

          logger.printBuffer(originalCommand);
          if (this.options.printOutput)
            logger.printEnd(result, this.options.minimalOutput || false);

          // 3. Save to Cache on Success
          if (result.success && hash && this.cacheManager && taskConfig.cache !== false) {
             const meta: CacheMeta = {
                 hash,
                 exitCode: exitCode || 0,
                 duration: result.duration,
                 timestamp: Date.now(),
                 stdout: result.output?.filter(o => o.type === 'stdout') || [],
                 stderr: result.output?.filter(o => o.type === 'stderr') || [],
             };
             
             const outputs = taskConfig.outputs || [];
             if (outputs.length > 0) {
                 await this.cacheManager.save(hash, outputs, meta);
             }
          }

          resolve(result);

      } catch (err) {
          result.endTime = new Date();
          result.error = err as Error;
          result.success = false;
          result.duration = result.endTime.getTime() - startTime.getTime();
          this.activeProcesses.delete(originalCommand);
          
          if (this.options.isServerMode) {
             const serverInfo = this.serverInfo.get(originalCommand);
             if (serverInfo) {
               serverInfo.status = 'error';
               this.serverInfo.set(originalCommand, serverInfo);
             }
             logger.printLine(`Command "${originalCommand}" failed to start: ${(err as Error).message}`, 'error');
          }
          
          logger.printBuffer(originalCommand);
          if (this.options.printOutput)
             logger.printEnd(result, this.options.minimalOutput || false);

          resolve(result);
      }
    });
  }

  private async readPackageJson(cwd: string): Promise<any> {
       const p = path.join(cwd, 'package.json');
       return await Bun.file(p).json(); 
  }

  async runSequential(commands: string[]): Promise<RunResult[]> {
    const results: RunResult[] = [];

    for (const cmd of commands) {
      const result = await this.runCommand(cmd);
      results.push(result);

      if (!result.success && this.options.stopOnError) {
        break;
      }
    }

    return results;
  }

  async runParallel(commands: string[]): Promise<RunResult[]> {
    const concurrency = this.options.maxParallel || commands.length;

    const mapper = async (cmd: string) => {
      return this.runCommand(cmd);
    };

    try {
      return await pMap(commands, mapper, {
        concurrency,
        stopOnError: this.options.stopOnError,
      });
    } catch (error) {
       // ...
      return [];
    }
  }

  async run(initialCommands: string[]): Promise<RunResult[]> {
    const commands = await this.expandWildcardCommands(initialCommands);
    if (commands.length === 0) {
      logger.printLine('No commands to run after wildcard expansion.', 'warn');
      return [];
    }

    logger.setCommands(commands);

    if (this.options.parallel) {
      if (this.options.isServerMode) {
        logger.printLine('Starting servers in parallel mode', 'info');
      }
      return this.runParallel(commands);
    } else {
      if (this.options.isServerMode) {
        logger.printLine('Starting servers in sequential mode', 'info');
      }
      return this.runSequential(commands);
    }
  }

  public cleanup(signal: NodeJS.Signals = 'SIGTERM'): void {
      // ... same cleanup
    if (this.isCleaningUp) return;
    this.isCleaningUp = true;

    logger.printLine('Cleaning up child processes...', 'warn');

    const promises: Promise<void>[] = [];

    this.activeProcesses.forEach((proc, command) => {
      if (proc.pid && !proc.killed) {
        promises.push(this.killProcess(proc, command, signal));
      }
    });

    // Wait for all processes to be killed
    Promise.all(promises)
      .then(() => {
        this.activeProcesses.clear();
        // Server info cleanup...
      })
      .catch(error => {
        logger.printLine(`Error during cleanup: ${error.message}`, 'error');
      });
  }

  private async killProcess(
    proc: Subprocess,
    command: string,
    signal: NodeJS.Signals
  ): Promise<void> {
    logger.printLine(`Killing ${command} (PID: ${proc.pid})...`, 'warn');

    return new Promise(resolve => {
        // Use process.kill to send specific signal
        if (proc.pid) {
             process.kill(proc.pid, signal);
        } else {
             proc.kill(); // Fallback
        }
        resolve();
    });
  }
}
