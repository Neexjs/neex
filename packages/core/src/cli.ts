// src/cli.ts - Main CLI file (refactored)
import { Command } from 'commander';
import {
  addRunCommands,
  addServerCommands,
  addDevCommands,
  addBuildCommands,
  addStartCommands,
  runInit,
  addPlugin,
} from './commands/index.js';
import chalk from 'chalk';
import figures from 'figures';

const { version } = require('../../package.json');

export default function cli(): void {
  const args = process.argv.slice(2);

  // Handle the 'init' command as a special case before anything else.
  // This makes 'neex' and 'neex init' act as aliases for 'npx create-neex'.
  if (args.length === 0 || args[0] === 'init') {
    const initArgs = args.slice(1); // Get all arguments after 'init'
    runInit(initArgs);
    return; // Exit early, do not proceed with the rest of the CLI
  }

  const program = new Command();

  // Initialize cleanup handlers
  const cleanupHandlers: Array<() => void | Promise<void>> = [];

  program
    .name('neex')
    .description(
      'Professional script runner with nodemon and PM2 functionality'
    )
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

  const devCommands = addDevCommands(program);
  cleanupHandlers.push(devCommands.cleanupDev);

  const buildCommands = addBuildCommands(program);
  cleanupHandlers.push(buildCommands.cleanupBuild);

  const startCommands = addStartCommands(program);
  cleanupHandlers.push(startCommands.cleanupStart);

  program.parse(process.argv);

  // Show help if no commands specified
  if (program.args.length === 0) {
    program.help();
  }

  // Graceful shutdown handling
  const handleSignal = async (signal: NodeJS.Signals) => {
    console.log(
      `\n${chalk.yellow(`${figures.warning} Received ${signal}. Cleaning up...`)}`
    );

    // Run all cleanup handlers
    for (const cleanup of cleanupHandlers) {
      try {
        await cleanup();
      } catch (error) {
        console.error(`Cleanup error:`, error);
      }
    }

    setTimeout(() => process.exit(0), 500);
  };

  process.on('SIGINT', () =>
    handleSignal('SIGINT').catch(err =>
      console.error('SIGINT handler error:', err)
    )
  );
  process.on('SIGTERM', () =>
    handleSignal('SIGTERM').catch(err =>
      console.error('SIGTERM handler error:', err)
    )
  );
  process.on('SIGQUIT', () =>
    handleSignal('SIGQUIT').catch(err =>
      console.error('SIGQUIT handler error:', err)
    )
  );
}
