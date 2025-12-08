/**
 * neexa build - Compile TypeScript to JavaScript
 */

import chalk from 'chalk';
import * as path from 'path';
import * as fs from 'fs';
import { spawn } from 'child_process';

interface BuildOptions {
    outdir: string;
    minify?: boolean;
    sourcemap?: boolean;
}

export async function buildCommand(file: string | undefined, options: BuildOptions): Promise<void> {
    const targetFile = file || 'src/index.ts';
    const outDir = options.outdir;

    if (!fs.existsSync(targetFile)) {
        console.error(chalk.red(`Error: File not found: ${targetFile}`));
        process.exit(1);
    }

    console.log(chalk.green('⚡ neexa build') + ` - Compiling ${chalk.cyan(targetFile)}`);

    const startTime = Date.now();

    // Ensure output directory exists
    if (!fs.existsSync(outDir)) {
        fs.mkdirSync(outDir, { recursive: true });
    }

    // Try bun build first, fallback to tsc
    const runtime = await detectBuildTool();
    
    const args = runtime.args.slice();
    
    if (runtime.tool === 'bun') {
        args.push(targetFile, '--outdir', outDir);
        if (options.minify) args.push('--minify');
        if (options.sourcemap) args.push('--sourcemap=external');
    } else {
        // tsc
        args.push('--outDir', outDir);
        if (options.sourcemap) args.push('--sourceMap');
    }

    const buildProcess = spawn(runtime.command, args, {
        stdio: 'inherit',
        env: { ...process.env, NODE_ENV: 'production' }
    });

    buildProcess.on('exit', (code) => {
        const duration = Date.now() - startTime;

        if (code === 0) {
            console.log(chalk.green('✓') + ` Built in ${duration}ms → ${outDir}/`);
        } else {
            console.error(chalk.red('✖') + ` Build failed with code ${code}`);
            process.exit(1);
        }
    });
}

async function detectBuildTool(): Promise<{ tool: string; command: string; args: string[] }> {
    try {
        const { execSync } = await import('child_process');
        execSync('bun --version', { stdio: 'ignore' });
        return { tool: 'bun', command: 'bun', args: ['build'] };
    } catch {
        return { tool: 'tsc', command: 'npx', args: ['tsc'] };
    }
}
