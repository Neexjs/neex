// src/commands/run-commands.ts - Sequential and parallel execution commands
import { Command } from 'commander';
import { run } from '../index';
import { Runner } from '../runner';
import { MonorepoManager } from '../monorepo';
import chalk from 'chalk';
import figures from 'figures';

export function addRunCommands(program: Command): void {

  // neex run <task>
  program
    .command('run <task>')
    .description('Run a task defined in neex.json across all workspaces')
    .action(async (task) => {
      try {
        const runner = new Runner({
            parallel: true, // managed by monorepo manager
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
        await monorepo.runTask(task);
        
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
    .command('p <commands...>', { isDefault: true })
    .alias('par')
    .alias('parallel')
    .description(
      'Run commands in parallel (default) or sequentially with -q. This is the default command.'
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

