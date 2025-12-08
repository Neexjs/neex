#!/usr/bin/env node
/**
 * neexa - Ultra-fast TypeScript Runner
 * 
 * Commands:
 *   neexa dev <file>    Start development server with hot-reload
 *   neexa build <file>  Compile TypeScript file
 *   neexa start <file>  Run production build
 */

import { Command } from 'commander';
import { devCommand } from './dev.js';
import { buildCommand } from './build.js';
import { startCommand } from './start.js';

const program = new Command();

program
    .name('neexa')
    .description('Ultra-fast TypeScript runner')
    .version('0.1.0');

// Dev command - hot reload
program
    .command('dev [file]')
    .description('Start development server with hot-reload')
    .option('-w, --watch <patterns>', 'Watch patterns', 'src/**/*')
    .option('-d, --delay <ms>', 'Restart delay', '100')
    .option('--fast', 'Ultra-fast mode (50ms delay)')
    .option('--inspect', 'Enable Node.js inspector')
    .option('--env <file>', 'Environment file', '.env')
    .action(devCommand);

// Build command - compile
program
    .command('build [file]')
    .description('Compile TypeScript file to JavaScript')
    .option('-o, --outdir <dir>', 'Output directory', 'dist')
    .option('--minify', 'Minify output')
    .option('--sourcemap', 'Generate source maps')
    .action(buildCommand);

// Start command - production
program
    .command('start [file]')
    .description('Run compiled file in production mode')
    .option('-p, --port <port>', 'Server port', '3000')
    .option('--cluster <workers>', 'Number of workers', '1')
    .action(startCommand);

program.parse();
