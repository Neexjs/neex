// src/logger.ts
import chalk from 'chalk';
import figures from 'figures';
import stringWidth from 'string-width';
import { CommandOutput, RunResult } from './types';
import { formatDuration } from './utils';

class Logger {
  private static instance: Logger;
  private prefixLength = 0;
  private outputBuffer: Map<string, CommandOutput[]> = new Map();
  private commandColors: Map<string, chalk.Chalk> = new Map();
  private startTimes: Map<string, Date> = new Map();
  private spinnerFrames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
  private spinnerIndex = 0;
  private spinnerIntervals: Map<string, NodeJS.Timeout> = new Map();
  private isSpinnerActive = false;

  private constructor() { }

  static getInstance(): Logger {
    if (!Logger.instance) {
      Logger.instance = new Logger();
    }
    return Logger.instance;
  }

  private getSpinnerFrame(): string {
    const frame = this.spinnerFrames[this.spinnerIndex];
    this.spinnerIndex = (this.spinnerIndex + 1) % this.spinnerFrames.length;
    return frame;
  }

  private showBanner(): void {
     console.log(
      '\n' + chalk.bgHex('#0066FF').black('         Neex         ') + '\n'
    );
  }

  setCommands(commands: string[]): void {
    // Clear any existing spinner intervals
    this.stopAllSpinners();

    // Show Neex banner
    this.showBanner();

    // Calculate prefix length for aligning output
    this.prefixLength = Math.max(...commands.map(cmd => stringWidth(cmd))) + 3;

    // Initialize buffers and colors for each command
    commands.forEach(cmd => {
      this.outputBuffer.set(cmd, []);
      this.commandColors.set(cmd, this.generateColor(cmd));
    });

    // Log commands that will be executed
    console.log(chalk.dim('» Commands to execute:'));
    commands.forEach(cmd => {
      const color = this.commandColors.get(cmd) || chalk.white;
      console.log(chalk.dim('  ┌') + color(` ${cmd}`));
    });
    console.log(''); // Add a blank line after commands list
  }

  private generateColor(command: string): chalk.Chalk {
    // Generate distinct colors for commands based on the command string
    const vibrantColors = [
      '#00BFFF', // Deep Sky Blue
      '#32CD32', // Lime Green
      '#FF6347', // Tomato
      '#9370DB', // Medium Purple
      '#FF8C00', // Dark Orange
      '#20B2AA', // Light Sea Green
      '#0066FF', // Deep Pink
      '#4169E1', // Royal Blue
      '#FFD700', // Gold
      '#8A2BE2'  // Blue Violet
    ];

    let hash = 0;
    for (let i = 0; i < command.length; i++) {
      hash = (hash << 5) - hash + command.charCodeAt(i);
      hash |= 0; // Convert to 32bit integer
    }

    const colorIndex = Math.abs(hash) % vibrantColors.length;
    return chalk.hex(vibrantColors[colorIndex]);
  }

  formatPrefix(command: string): string {
    const color = this.commandColors.get(command) || chalk.white;
    const prefix = `${command}:`.padEnd(this.prefixLength);
    return color(prefix);
  }

  bufferOutput(output: CommandOutput): void {
    const currentBuffer = this.outputBuffer.get(output.command) || [];
    currentBuffer.push(output);
    this.outputBuffer.set(output.command, currentBuffer);
  }

  printBuffer(command: string): void {
    const buffer = this.outputBuffer.get(command) || [];
    const color = this.commandColors.get(command) || chalk.white;

    // Stop spinner for this command if running
    this.stopSpinner(command);

    buffer.forEach(output => {
      const prefix = this.formatPrefix(output.command);
      const content = output.data.trim();

      if (content) {
        const lines = content.split('\n');
        lines.forEach(line => {
          if (line.trim()) {
            const outputLine = `${prefix} ${line}`;
            // Show stderr in appropriate colors
            if (output.type === 'stderr') {
              // Not all stderr is an error, check for warning or info patterns
              if (line.toLowerCase().includes('warn') || line.toLowerCase().includes('warning')) {
                console.log(`${prefix} ${chalk.yellow(line)}`);
              } else if (line.toLowerCase().includes('error')) {
                console.log(`${prefix} ${chalk.red(line)}`);
              } else {
                console.log(`${prefix} ${line}`);
              }
            } else {
              console.log(outputLine);
            }
          }
        });
      }
    });

    // Clear buffer after printing
    this.outputBuffer.set(command, []);
  }

  clearBuffer(command: string): void {
    this.outputBuffer.set(command, []);
  }

  printLine(message: string, level: 'info' | 'error' | 'warn' = 'info'): void {
    if (level === 'error') {
      console.error(chalk.red(`${figures.cross} ${message}`));
    } else if (level === 'warn') {
      console.warn(chalk.yellow(`${figures.warning} ${message}`));
    } else {
      console.log(chalk.blue(`${figures.info} ${message}`));
    }
  }

  printStart(command: string): void {
    // Record start time
    this.startTimes.set(command, new Date());

    const prefix = this.formatPrefix(command);
    const color = this.commandColors.get(command) || chalk.white;

    // Stop any previous spinner for this command (e.g. if retrying)
    this.stopSpinner(command);
    // Clear the line before printing "Starting..."
    if (this.isSpinnerActive) { // Check if any spinner was active to avoid clearing unnecessarily
        process.stdout.write('\r' + ' '.repeat(process.stdout.columns || 80) + '\r');
    }

    console.log(`${prefix} ${color('Starting...')}`);

    // Start spinner for this command
    this.startSpinner(command);
  }

  startSpinner(command: string): void {
    // Only create a spinner if one doesn't already exist for this command
    if (this.spinnerIntervals.has(command)) {
      return;
    }

    this.isSpinnerActive = true;
    const color = this.commandColors.get(command) || chalk.white;
    const prefix = this.formatPrefix(command);

    const interval = setInterval(() => {
      const frame = this.getSpinnerFrame();
      process.stdout.write(`\r${prefix} ${color(frame)} ${chalk.dim('Running...')}`);
    }, 80);

    this.spinnerIntervals.set(command, interval);
  }

  stopSpinner(command: string): void {
    const interval = this.spinnerIntervals.get(command);
    if (interval) {
      clearInterval(interval);
      this.spinnerIntervals.delete(command);

      // Clear the spinner line
      if (this.isSpinnerActive) {
        process.stdout.write('\r' + ' '.repeat(process.stdout.columns || 80) + '\r');
      }
    }
  }

  stopAllSpinners(): void {
    this.spinnerIntervals.forEach((interval, command) => {
      clearInterval(interval);
    });
    this.spinnerIntervals.clear();
    this.isSpinnerActive = false;

    // Clear the spinner line if any spinner was active
    if (this.isSpinnerActive) {
      process.stdout.write('\r' + ' '.repeat(process.stdout.columns || 80) + '\r');
    }
  }

  printSuccess(result: RunResult): void {
    const { command, duration } = result;
    this.stopSpinner(command);

    const prefix = this.formatPrefix(command);
    const color = this.commandColors.get(command) || chalk.white;
    const durationStr = duration
      ? ` ${chalk.dim(`(${(duration / 1000).toFixed(2)}s)`)}`
      : '';

    console.log(`${prefix} ${chalk.green(figures.tick)} ${chalk.green('Completed')}${durationStr}`);
  }

  printError(result: RunResult): void {
    const { command, error, code, duration } = result;
    this.stopSpinner(command);

    const prefix = this.formatPrefix(command);
    const durationStr = duration ? ` ${chalk.dim(`(${(duration / 1000).toFixed(2)}s)`)}` : '';
    const errorCode = code !== null ? ` ${chalk.red(`[code: ${code}]`)}` : '';

    console.error(`${prefix} ${chalk.red(figures.cross)} ${chalk.red('Failed')}${errorCode}${durationStr}`);

    if (error) {
      console.error(`${prefix} ${chalk.red(error.message)}`);
    }
  }

  printEnd(result: RunResult, minimalOutput: boolean): void {
    this.stopSpinner(result.command);
    const prefix = this.formatPrefix(result.command); // Corrected to formatPrefix
    
    let durationDisplay = '';
    if (result.duration !== null) {
      // Ensure result.duration is treated as a number here
      durationDisplay = `(${formatDuration(result.duration as number)})`;
    }
    const duration = durationDisplay;

    if (minimalOutput) {
      if (!result.success) {
        const status = result.code !== null ? `failed (code ${result.code})` : 'failed';
        this.printLine(`${prefix} ${chalk.red(figures.cross)} ${result.command} ${status} ${duration}`, 'error');
      }
    } else {
      if (result.success) {
        this.printLine(`${prefix} ${chalk.green(figures.tick)} Command "${result.command}" finished successfully ${duration}`, 'info');
      } else {
        const errorCode = result.code !== null ? ` (code ${result.code})` : '';
        const errorMessage = result.error ? `: ${result.error.message}` : '';
        this.printLine(`${prefix} ${chalk.red(figures.cross)} Command "${result.command}" failed${errorCode}${errorMessage} ${duration}`, 'error');
      }
    }
  }

  printSummary(results: RunResult[]): void {
    // Stop any remaining spinners
    this.stopAllSpinners();

    const successful = results.filter(r => r.success).length;
    const failed = results.length - successful;
    const totalDuration = results.reduce((sum, result) => sum + (result.duration || 0), 0);
    const totalSeconds = (totalDuration / 1000).toFixed(2);

    console.log(
      '\n' + chalk.bgHex('#0066FF').black('         Execution Summary         ') + '\n'
    );

    console.log(`${chalk.green(`${figures.tick} ${successful} succeeded`)}, ${chalk.red(`${figures.cross} ${failed} failed`)}`);
    console.log(`${chalk.blue(figures.info)} ${chalk.dim(`Total execution time: ${totalSeconds}s`)}`);

    if (successful > 0) {
      console.log('\n' + chalk.green.bold('Successful commands:'));
      results
        .filter(r => r.success)
        .forEach(result => {
          const color = this.commandColors.get(result.command) || chalk.white;
          const duration = result.duration
            ? chalk.dim(` (${(result.duration / 1000).toFixed(2)}s)`)
            : '';
          console.log(`  ${chalk.green(figures.tick)} ${color(result.command)}${duration}`);
        });
    }

    if (failed > 0) {
      console.log('\n' + chalk.red.bold('Failed commands:'));
      results
        .filter(r => !r.success)
        .forEach(result => {
          const color = this.commandColors.get(result.command) || chalk.white;
          const duration = result.duration
            ? chalk.dim(` (${(result.duration / 1000).toFixed(2)}s)`)
            : '';
          const code = result.code !== null ? chalk.red(` [code: ${result.code}]`) : '';
          console.log(`  ${chalk.red(figures.cross)} ${color(result.command)}${code}${duration}`);
        });
    }
  }
}

export default Logger.getInstance();