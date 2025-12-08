// src/commands/run-commands.ts - Sequential and parallel execution commands
import { Command } from 'commander';
import { run } from '../index';
import { Runner } from '../runner';
import { MonorepoManager } from '../monorepo';
import chalk from 'chalk';
import figures from 'figures';

export async function runTask(task: string, options: any = {}) {
    const { Runner } = require('../runner');
    const { MonorepoManager } = require('../monorepo');
    const chalk = require('chalk');
    const figures = require('figures');

    try {
        const runner = new Runner({
            parallel: options.parallel ?? true,
            printOutput: options.printOutput ?? true,
            color: options.color ?? true,
            showTiming: options.showTiming ?? true,
            prefix: options.prefix ?? true,
            stopOnError: options.stopOnError ?? true,
            minimalOutput: options.minimalOutput ?? false,
            groupOutput: options.groupOutput ?? false,
            isServerMode: options.isServerMode ?? false
        });
        
        const monorepo = new MonorepoManager(process.cwd(), runner);
        await monorepo.runTask(task);
        
    } catch (error: unknown) {
        if (error instanceof Error) {
            console.error(chalk.red(`${figures.cross} Error: ${error.message}`));
        } else {
            console.error(chalk.red(`${figures.cross} An unknown error occurred`));
        }
        process.exit(1);
    }
}

export function addRunCommands(program: Command): void {

  // neex run <task>
  program
    .command('run <task>')
    .description('Run a task defined in neex.json across all workspaces')
    .action(async (task) => {
        await runTask(task);
    });

  // neex affected <task> - Only run on changed packages
  program
    .command('affected <task>')
    .description('Run a task only on packages affected by git changes')
    .option('-b, --base <ref>', 'Git ref to compare against', 'HEAD~1')
    .action(async (task, options) => {
      try {
        const runner = new Runner({
            parallel: true,
            printOutput: true,
            color: true,
            showTiming: true,
            prefix: true,
            stopOnError: true,
            minimalOutput: false,
            groupOutput: false,
            isServerMode: false
        });
        
        const monorepo = new MonorepoManager(process.cwd(), runner);
        await monorepo.runAffected(task, options.base);
        
      } catch (error: unknown) {
        if (error instanceof Error) {
          console.error(chalk.red(`${figures.cross} Error: ${error.message}`));
        } else {
          console.error(chalk.red(`${figures.cross} An unknown error occurred`));
        }
        process.exit(1);
      }
    });

  // Main command for sequential execution (similar to run-s)
  program
    .command('s <commands...>')
    .alias('seq')
    .alias('sequential')
    .description('Run commands sequentially')
    .option('-c, --no-color', 'Disable colored output')
    .option('-t, --no-timing', 'Hide timing information')
    .option('-p, --no-prefix', 'Hide command prefix')
    .option('-s, --stop-on-error', 'Stop on first error')
    .option('-o, --no-output', 'Hide command output')
    .option('-m, --minimal', 'Use minimal output format')
    .action(async (commands, options) => {
      try {
        await run(commands, {
          parallel: false,
          color: options.color,
          showTiming: options.timing,
          prefix: options.prefix,
          stopOnError: options.stopOnError,
          printOutput: options.output,
          minimalOutput: options.minimal,
        });
      } catch (error: unknown) {
        if (error instanceof Error) {
          console.error(chalk.red(`${figures.cross} Error: ${error.message}`));
        } else {
          console.error(
            chalk.red(`${figures.cross} An unknown error occurred`)
          );
        }
        process.exit(1);
      }
    });

  // runx command: parallel execution by default (with alias 'p'), can run sequentially with -q
  program
    .command('p <commands...>')
    .alias('par')
    .alias('parallel')
    .description(
      'Run commands in parallel (default) or sequentially with -q'
    )
    .option('-c, --no-color', 'Disable colored output')
    .option('-t, --no-timing', 'Hide timing information')
    .option('-p, --no-prefix', 'Hide command prefix')
    .option('-s, --stop-on-error', 'Stop on first error')
    .option('-o, --no-output', 'Hide command output')
    .option('-m, --minimal', 'Use minimal output format')
    .option(
      '-x, --max-parallel <number>',
      'Maximum number of parallel processes',
      parseInt
    )
    .option(
      '-q, --sequential',
      'Run commands sequentially instead of in parallel'
    )
    .option(
      '--retry <count>',
      'Number of times to retry a failed command',
      parseInt
    )
    .option(
      '--retry-delay <ms>',
      'Delay in milliseconds between retries',
      parseInt
    )
    .action(async (commands, options) => {
      try {
        await run(commands, {
          parallel: !options.sequential,
          maxParallel: options.maxParallel,
          color: options.color,
          showTiming: options.timing,
          prefix: options.prefix,
          stopOnError: options.stopOnError,
          printOutput: options.output,
          minimalOutput: options.minimal,
          retry: options.retry,
          retryDelay: options.retryDelay,
        });
      } catch (error: unknown) {
        if (error instanceof Error) {
          console.error(chalk.red(`${figures.cross} Error: ${error.message}`));
        } else {
          console.error(
            chalk.red(`${figures.cross} An unknown error occurred`)
          );
        }
        process.exit(1);
      }
    });
}

