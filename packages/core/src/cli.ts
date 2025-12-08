// src/cli.ts - neex CLI (Monorepo Orchestrator Only)
import { Command } from 'commander';
import {
  addRunCommands,
  addServerCommands,
  addCacheCommands,
  runInit,
  addPlugin,
} from './commands/index.js';
import chalk from 'chalk';
import figures from 'figures';

const { version } = require('../../package.json');

export default function cli(): void {
  const args = process.argv.slice(2);

  // Handle the 'init' command as a special case
  if (args.length === 0 || args[0] === 'init') {
    const initArgs = args.slice(1);
    runInit(initArgs);
    return;
  }

  const program = new Command();

  program
    .name('neex')
    .description('Monorepo orchestrator - task runner, build tool, and remote cache')
    .version(version);

  // Add plugin command
  program
    .command('add <plugin>')
    .description('Add a plugin to the project')
    .action(async (plugin: string) => {
      await addPlugin(plugin);
    });

  // Add all other command groups
  addRunCommands(program);
  addServerCommands(program);
  addCacheCommands(program);

  // Catch-all: treat unknown commands as tasks (e.g., neex build -> neex run build)
  program
    .arguments('[task]')
    .action(async (task) => {
      // If task is not one of the known commands, run it
      if (task) {
        // We import the run logic dynamically to avoid circular deps if any
        const { runTask } = require('./commands/run-commands.js');
        await runTask(task, {
            parallel: true,
            printOutput: true,
            color: true,
            showTiming: true,
            prefix: true,
            stopOnError: true,
        });
      } else {
        program.help();
      }
    });

  program.parse(process.argv);

  // Graceful shutdown
  process.on('SIGINT', () => {
    console.log(`\n${chalk.yellow(`${figures.warning} Received SIGINT. Exiting...`)}`);
    process.exit(0);
  });

  process.on('SIGTERM', () => {
    console.log(`\n${chalk.yellow(`${figures.warning} Received SIGTERM. Exiting...`)}`);
    process.exit(0);
  });
}
