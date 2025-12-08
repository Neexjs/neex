/**
 * neexa start - Run production build
 */

import { spawn, ChildProcess } from 'child_process';
import chalk from 'chalk';
import * as path from 'path';
import * as fs from 'fs';

interface StartOptions {
    port: string;
    cluster: string;
}

export async function startCommand(file: string | undefined, options: StartOptions): Promise<void> {
    const targetFile = file || 'dist/index.js';
    const port = options.port;
    const workers = parseInt(options.cluster) || 1;

    // Check if file exists
    if (!fs.existsSync(targetFile)) {
        console.error(chalk.red(`Error: File not found: ${targetFile}`));
        console.log(chalk.yellow('Hint: Run `neexa build` first'));
        process.exit(1);
    }

    console.log(chalk.green('🚀 neexa start') + ` - Running ${chalk.cyan(targetFile)}`);
    console.log(chalk.gray(`   Port: ${port} | Workers: ${workers}`));

    // Detect runtime
    const runtime = await detectRuntime();

    const proc = spawn(runtime.command, [...runtime.args, targetFile], {
        stdio: 'inherit',
        env: {
            ...process.env,
            NODE_ENV: 'production',
            PORT: port,
            WORKERS: workers.toString()
        }
    });

    proc.on('exit', (code) => {
        if (code !== 0) {
            console.error(chalk.red('✖') + ` Process exited with code ${code}`);
        }
        process.exit(code || 0);
    });

    // Graceful shutdown
    process.on('SIGINT', () => {
        console.log('\n' + chalk.yellow('⏹') + ' Shutting down...');
        proc.kill('SIGTERM');
    });

    process.on('SIGTERM', () => {
        proc.kill('SIGTERM');
    });
}

async function detectRuntime(): Promise<{ command: string; args: string[] }> {
    try {
        const { execSync } = await import('child_process');
        execSync('bun --version', { stdio: 'ignore' });
        return { command: 'bun', args: ['run'] };
    } catch {
        return { command: 'node', args: [] };
    }
}
