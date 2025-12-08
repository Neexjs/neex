// src/commands/server-commands.ts - Server-specific commands
import { Command } from 'commander';
import { run } from '../index';
import chalk from 'chalk';
import figures from 'figures';

export function addServerCommands(program: Command): void {

  // Servers command specifically optimized for running web servers
  program
    .command('servers <commands...>')
    .alias('srv')
    .description(
      'Run multiple servers with optimized output for API, frontend, etc.'
    )
    .option('-c, --no-color', 'Disable colored output')
    .option('-t, --no-timing', 'Hide timing information')
    .option('-p, --no-prefix', 'Hide command prefix')
    .option('-s, --stop-on-error', 'Stop when any server crashes')
    .option(
      '-x, --max-parallel <number>',
      'Maximum number of parallel servers',
      parseInt
    )
    .option('-g, --group-output', 'Group outputs by server')
    .action(async (commands, options) => {
      try {
        console.log(
          chalk.blue(`${figures.info} Starting servers in parallel mode...`)
        );

        await run(commands, {
          parallel: true,
          maxParallel: options.maxParallel,
          color: options.color,
          showTiming: options.timing,
          prefix: options.prefix,
          stopOnError: options.stopOnError,
          printOutput: true,
          groupOutput: options.groupOutput,
          isServerMode: true,
        });
      } catch (error: unknown) {
        if (error instanceof Error) {
          console.error(
            chalk.red(`${figures.cross} Server Error: ${error.message}`)
          );
        } else {
          console.error(
            chalk.red(`${figures.cross} An unknown server error occurred`)
          );
        }
        process.exit(1);
      }
    });
}
