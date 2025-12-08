import chalk from 'chalk';

export const loggerManager = {
  printLine: (message: string, level: 'info' | 'warn' | 'error' = 'info') => {
    const prefix = {
      info: chalk.blue('i'),
      warn: chalk.yellow('!'),
      error: chalk.red('x'),
    }[level];

    console.log(`${prefix} ${message}`);
  },
};
