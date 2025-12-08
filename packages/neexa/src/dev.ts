/**
 * neexa dev - Development server with hot-reload
 */

import { spawn, ChildProcess } from 'child_process';
import { watch, FSWatcher } from 'chokidar';
import chalk from 'chalk';
import * as path from 'path';
import * as fs from 'fs';

interface DevOptions {
    watch: string;
    delay: string;
    fast?: boolean;
    inspect?: boolean;
    env?: string;
}

let currentProcess: ChildProcess | null = null;
let watcher: FSWatcher | null = null;
let restartTimeout: NodeJS.Timeout | null = null;

export async function devCommand(file: string | undefined, options: DevOptions): Promise<void> {
    const targetFile = file || 'src/index.ts';
    const delay = options.fast ? 50 : parseInt(options.delay) || 100;

    if (!fs.existsSync(targetFile)) {
        console.error(chalk.red(`Error: File not found: ${targetFile}`));
        process.exit(1);
    }

    console.log(chalk.green('⚡ neexa dev') + ` - Starting ${chalk.cyan(targetFile)}`);

    // Load env file
    if (options.env && fs.existsSync(options.env)) {
        loadEnvFile(options.env);
    }

    // Start initial process
    await startProcess(targetFile, options);

    // Setup watcher
    const watchPatterns = options.watch.split(',').map(p => p.trim());
    
    watcher = watch(watchPatterns, {
        ignored: ['node_modules/**', '.git/**', 'dist/**', '**/*.d.ts'],
        ignoreInitial: true,
        awaitWriteFinish: { stabilityThreshold: 50 }
    });

    watcher.on('change', (changedFile) => {
        console.log(chalk.blue('●') + ` ${path.relative(process.cwd(), changedFile)}`);
        debounceRestart(targetFile, options, delay);
    });

    watcher.on('add', (addedFile) => {
        console.log(chalk.green('+') + ` ${path.relative(process.cwd(), addedFile)}`);
        debounceRestart(targetFile, options, delay);
    });

    console.log(chalk.green('✓') + ' Watching for changes...\n');

    // Cleanup on exit
    process.on('SIGINT', cleanup);
    process.on('SIGTERM', cleanup);
}

function debounceRestart(file: string, options: DevOptions, delay: number): void {
    if (restartTimeout) clearTimeout(restartTimeout);
    restartTimeout = setTimeout(() => restart(file, options), delay);
}

async function restart(file: string, options: DevOptions): Promise<void> {
    console.log(chalk.yellow('⟳') + ' Restarting...\n');
    await stopProcess();
    await startProcess(file, options);
}

async function startProcess(file: string, options: DevOptions): Promise<void> {
    const args: string[] = [];
    
    if (options.inspect) {
        args.push('--inspect');
    }
    
    args.push(file);

    // Use bun if available, otherwise node with tsx
    const runtime = await detectRuntime();
    
    currentProcess = spawn(runtime.command, [...runtime.args, ...args], {
        stdio: 'inherit',
        env: {
            ...process.env,
            NODE_ENV: 'development',
            FORCE_COLOR: '1'
        }
    });

    currentProcess.on('exit', (code, signal) => {
        if (code !== 0 && signal !== 'SIGTERM') {
            console.log(chalk.red('✖') + ` Process exited with code ${code}`);
        }
        currentProcess = null;
    });
}

async function stopProcess(): Promise<void> {
    if (!currentProcess) return;

    return new Promise((resolve) => {
        const proc = currentProcess!;
        currentProcess = null;

        proc.on('exit', () => resolve());
        proc.kill('SIGTERM');

        setTimeout(() => {
            if (!proc.killed) proc.kill('SIGKILL');
            resolve();
        }, 1000);
    });
}

async function detectRuntime(): Promise<{ command: string; args: string[] }> {
    // Try bun first
    try {
        const { execSync } = await import('child_process');
        execSync('bun --version', { stdio: 'ignore' });
        return { command: 'bun', args: ['run'] };
    } catch {
        // Fallback to node with tsx
        return { command: 'npx', args: ['tsx'] };
    }
}

function loadEnvFile(envFile: string): void {
    try {
        const content = fs.readFileSync(envFile, 'utf-8');
        const lines = content.split('\n');
        let count = 0;

        for (const line of lines) {
            const trimmed = line.trim();
            if (!trimmed || trimmed.startsWith('#')) continue;

            const [key, ...valueParts] = trimmed.split('=');
            if (key && valueParts.length > 0) {
                const value = valueParts.join('=').replace(/^["']|["']$/g, '');
                process.env[key.trim()] = value;
                count++;
            }
        }

        console.log(chalk.green('✓') + ` Loaded ${count} env variables from ${envFile}`);
    } catch (error) {
        // Ignore
    }
}

function cleanup(): void {
    console.log('\n' + chalk.yellow('⏹') + ' Shutting down...');
    
    if (restartTimeout) clearTimeout(restartTimeout);
    if (watcher) watcher.close();
    if (currentProcess) currentProcess.kill('SIGTERM');
    
    process.exit(0);
}
