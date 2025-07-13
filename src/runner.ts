// src/runner.ts - Updated version
import { spawn, ChildProcess } from 'child_process';
import * as fsPromises from 'fs/promises';
import * as path from 'path';
import chalk from 'chalk';
import { RunOptions, RunResult, CommandOutput, ServerInfo } from './types';
import logger from './logger';
import pMap from 'p-map';
import npmRunPath from 'npm-run-path';
import * as fs from 'fs';

export class Runner {
    private options: RunOptions;
    private activeProcesses: Map<string, ChildProcess> = new Map();
    private serverInfo: Map<string, ServerInfo> = new Map();
    private portRegex = /listening on (?:port |http:\/\/localhost:|https:\/\/localhost:)(\d+)/i;
    private urlRegex = /(https?:\/\/localhost:[0-9]+(?:\/[^\s]*)?)/i;

    constructor(options: RunOptions) {
        this.options = options;
        this.activeProcesses = new Map();
    }

    private async expandWildcardCommands(commands: string[]): Promise<string[]> {
        const expandedCommands: string[] = [];
        let packageJson: any;
        try {
            const packageJsonPath = path.join(process.cwd(), 'package.json');
            if (fs.existsSync(packageJsonPath)) {
                const packageJsonContent = await fsPromises.readFile(packageJsonPath, 'utf-8');
                packageJson = JSON.parse(packageJsonContent);
            }
        } catch (error) {
            logger.printLine(`Could not read or parse package.json: ${(error as Error).message}`, 'warn');
            packageJson = { scripts: {} };
        }

        for (const command of commands) {
            if (command.includes('*') && packageJson && packageJson.scripts) {
                const pattern = new RegExp(`^${command.replace(/\*/g, '.*')}$`);
                let foundMatch = false;
                for (const scriptName in packageJson.scripts) {
                    if (pattern.test(scriptName)) {
                        expandedCommands.push(scriptName); // Or packageJson.scripts[scriptName] if you want the script value
                        foundMatch = true;
                    }
                }
                if (!foundMatch) {
                    logger.printLine(`No scripts found in package.json matching wildcard: ${command}`, 'warn');
                    expandedCommands.push(command); // Add original command if no match
                }
            } else {
                expandedCommands.push(command);
            }
        }
        return expandedCommands;
    }

    private async resolveScriptAndCwd(scriptNameOrCommand: string, baseDir: string): Promise<{ executableCommand: string, executionCwd?: string }> {
        try {
            const packageJsonPath = path.join(baseDir, 'package.json');
            const packageJsonContent = await fsPromises.readFile(packageJsonPath, 'utf-8');
            const packageJson = JSON.parse(packageJsonContent);

            if (packageJson.scripts && packageJson.scripts[scriptNameOrCommand]) {
                const scriptValue: string = packageJson.scripts[scriptNameOrCommand];

                const cdMatch = scriptValue.match(/^cd\s+([^&]+)\s+&&\s+(.*)$/);
                if (cdMatch) {
                    const dir = cdMatch[1];
                    const commandToExecute = cdMatch[2];
                    const targetCwd = path.resolve(baseDir, dir);
                    return { executableCommand: commandToExecute, executionCwd: targetCwd };
                } else {
                    // It's a script from package.json, but no 'cd ... && ...' pattern
                    return { executableCommand: scriptValue, executionCwd: baseDir };
                }
            }
        } catch (error) {
            // Errors like package.json not found, or script not in package.json
            // Will treat as direct command
        }

        return { executableCommand: scriptNameOrCommand, executionCwd: undefined };
    }

    detectServerInfo(command: string, data: string): void {
        if (!this.options.isServerMode) return;

        // Get or create server info
        let serverInfo = this.serverInfo.get(command);
        if (!serverInfo) {
            serverInfo = {
                name: command,
                status: 'starting'
            };
            this.serverInfo.set(command, serverInfo);
        }

        // Try to detect port from output
        const portMatch = data.match(this.portRegex);
        if (portMatch && portMatch[1]) {
            serverInfo.port = parseInt(portMatch[1], 10);
            serverInfo.status = 'running';

            // Only log if we just discovered the port
            if (!serverInfo.url) {
                logger.printLine(`Server ${command} running on port ${serverInfo.port}`, 'info');
            }
        }

        // Try to detect full URL from output
        const urlMatch = data.match(this.urlRegex);
        if (urlMatch && urlMatch[1]) {
            serverInfo.url = urlMatch[1];
            serverInfo.status = 'running';

            // Log the full URL once we detect it
            logger.printLine(`Server ${command} available at ${chalk.cyan(serverInfo.url)}`, 'info');
        }

        // Update server info
        this.serverInfo.set(command, serverInfo);
    }

    async runCommand(originalCommand: string, currentRetry = 0): Promise<RunResult> {
        const { executableCommand: command, executionCwd: cwd } = await this.resolveScriptAndCwd(originalCommand, process.cwd());
        const startTime = new Date();
        const result: RunResult = {
            command: originalCommand,
            success: false,
            code: null,
            startTime,
            endTime: null,
            output: [],
            stderr: []
        };

        if (this.options.printOutput) {
            logger.printStart(originalCommand);
        }

        return new Promise<RunResult>(async (resolve) => { 
            const [cmd, ...args] = command.split(' ');
            const env = {
                ...process.env,
                ...npmRunPath.env(),
                FORCE_COLOR: this.options.color ? '1' : '0'
            };

            const proc = spawn(cmd, args, {
                stdio: ['ignore', 'pipe', 'pipe'],
                shell: true,
                env,
                detached: true, 
                cwd
            });

            this.activeProcesses.set(originalCommand, proc);

            if (this.options.isServerMode) {
                this.serverInfo.set(originalCommand, {
                    name: originalCommand,
                    status: 'starting',
                    pid: proc.pid,
                    startTime: new Date()
                });
            }

            proc.stdout?.on('data', (data) => {
                const output: CommandOutput = {
                    command: originalCommand,
                    type: 'stdout',
                    data: data.toString(),
                    timestamp: new Date()
                };
                if (this.options.isServerMode) this.detectServerInfo(originalCommand, data.toString());
                if (result.output) result.output.push(output);
                logger.bufferOutput(output);
                if (!this.options.groupOutput && this.options.printOutput) logger.printBuffer(originalCommand);
            });

            proc.stderr?.on('data', (data) => {
                const output: CommandOutput = {
                    command: originalCommand,
                    type: 'stderr',
                    data: data.toString(),
                    timestamp: new Date()
                };
                if (result.output) result.output.push(output);
                logger.bufferOutput(output);
                if (!this.options.groupOutput && this.options.printOutput) logger.printBuffer(originalCommand);
            });

            proc.on('error', async (err) => { 
                result.endTime = new Date();
                result.error = err;
                result.success = false;
                result.duration = result.endTime.getTime() - startTime.getTime();
                this.activeProcesses.delete(originalCommand);

                if (this.options.isServerMode) {
                    const serverInfo = this.serverInfo.get(originalCommand);
                    if (serverInfo) {
                        serverInfo.status = 'error';
                        this.serverInfo.set(originalCommand, serverInfo);
                    }
                    logger.printLine(`Command "${originalCommand}" failed to start: ${err.message}`, 'error');
                }
                
                logger.printBuffer(originalCommand); 
                if (this.options.printOutput) logger.printEnd(result, this.options.minimalOutput);

                if (this.options.retry && this.options.retry > 0 && currentRetry < this.options.retry) {
                    logger.printLine(
                        `Command "${originalCommand}" failed with error. Retrying (${currentRetry + 1}/${this.options.retry})...`,
                        'warn'
                    );
                    if (this.options.retryDelay && this.options.retryDelay > 0) {
                        await new Promise(res => setTimeout(res, this.options.retryDelay));
                    }
                    logger.clearBuffer(originalCommand);
                    resolve(this.runCommand(originalCommand, currentRetry + 1));
                } else {
                    resolve(result);
                }
            });

            proc.on('close', async (code) => { 
                result.code = code;
                result.success = code === 0;
                result.endTime = new Date();
                result.duration = result.endTime.getTime() - startTime.getTime();
                this.activeProcesses.delete(originalCommand);

                if (this.options.isServerMode) {
                    const serverInfo = this.serverInfo.get(originalCommand);
                    if (serverInfo) {
                        serverInfo.status = code === 0 ? 'stopped' : 'error';
                        this.serverInfo.set(originalCommand, serverInfo);
                    }
                    if (code !== 0) {
                        logger.printLine(`Server "${originalCommand}" exited with code ${code}`, 'error');
                    }
                }

                logger.printBuffer(originalCommand); 
                if (this.options.printOutput) logger.printEnd(result, this.options.minimalOutput);

                if (!result.success && this.options.retry && this.options.retry > 0 && currentRetry < this.options.retry) {
                    logger.printLine(
                        `Command "${originalCommand}" failed with code ${code}. Retrying (${currentRetry + 1}/${this.options.retry})...`,
                        'warn'
                    );
                    if (this.options.retryDelay && this.options.retryDelay > 0) {
                        await new Promise(res => setTimeout(res, this.options.retryDelay));
                    }
                    logger.clearBuffer(originalCommand);
                    resolve(this.runCommand(originalCommand, currentRetry + 1));
                } else {
                    resolve(result);
                }
            });
        });
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
                stopOnError: this.options.stopOnError
            });
        } catch (error) {
            if (this.options.isServerMode) {
                logger.printLine('One or more servers failed to start. Stopping all servers.', 'error');
            }
            return [];
        }
    }

    async run(initialCommands: string[]): Promise<RunResult[]> {
        const commands = await this.expandWildcardCommands(initialCommands);
        if (commands.length === 0) {
            logger.printLine('No commands to run after wildcard expansion.', 'warn');
            return [];
        }
        // Initialize logger with the final list of commands
        logger.setCommands(commands);

        // Run in parallel or sequential mode
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
        logger.printLine('Cleaning up child processes...', 'warn');

        this.activeProcesses.forEach((proc, command) => {
            if (proc.pid && !proc.killed) {
                try {
                    // Kill process group
                    process.kill(-proc.pid, signal);
                    logger.printLine(`Sent ${signal} to process group ${proc.pid} (${command})`, 'info');
                } catch (e) {
                    // Fallback if killing group failed
                    try {
                        proc.kill(signal);
                        logger.printLine(`Sent ${signal} to process ${proc.pid} (${command})`, 'info');
                    } catch (errInner) {
                        logger.printLine(`Failed to kill process ${proc.pid} (${command}): ${(errInner as Error).message}`, 'error');
                    }
                }
            }
        });

        this.activeProcesses.clear();

        // Print server status summary if in server mode
        if (this.options.isServerMode && this.serverInfo.size > 0) {
            logger.printLine('Server shutdown summary:', 'info');
            this.serverInfo.forEach((info, command) => {
                const statusColor = info.status === 'running' ? chalk.green :
                    info.status === 'error' ? chalk.red : chalk.yellow;
                logger.printLine(`  ${command}: ${statusColor(info.status)}`, 'info');
            });
        }
    }
}