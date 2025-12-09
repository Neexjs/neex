/**
 * Neex Terminal UI (TUI)
 * 
 * Turbo-style interactive terminal interface:
 * - Left panel: Task list with status
 * - Right panel: Selected task output
 * - Top: NEEX logo
 * - Bottom: Keyboard shortcuts
 * 
 * Uses Bun's native TTY for maximum performance
 */

import chalk from 'chalk';
import figures from 'figures';

// ============================================================================
// Types
// ============================================================================

export interface TUITask {
  name: string;
  command: string;
  status: 'pending' | 'running' | 'success' | 'error';
  output: string[];
  startTime?: number;
  endTime?: number;
  exitCode?: number;
}

export interface TUIOptions {
  title?: string;
  showLogo?: boolean;
  interactive?: boolean;
}

// ============================================================================
// ANSI Escape Codes
// ============================================================================

const ESC = '\x1b';
const CSI = `${ESC}[`;

const ansi = {
  clearScreen: `${CSI}2J`,
  moveTo: (row: number, col: number) => `${CSI}${row};${col}H`,
  moveToTop: `${CSI}1;1H`,
  hideCursor: `${CSI}?25l`,
  showCursor: `${CSI}?25h`,
  saveCursor: `${ESC}7`,
  restoreCursor: `${ESC}8`,
  clearLine: `${CSI}2K`,
  bold: `${CSI}1m`,
  dim: `${CSI}2m`,
  reset: `${CSI}0m`,
  inverse: `${CSI}7m`,
};

// ============================================================================
// Terminal UI Class
// ============================================================================

export class TerminalUI {
  private tasks: Map<string, TUITask> = new Map();
  private selectedIndex: number = 0;
  private scrollOffset: number = 0;
  private running: boolean = false;
  private options: TUIOptions;
  private termWidth: number = 80;
  private termHeight: number = 24;
  private leftPanelWidth: number = 25;

  constructor(options: TUIOptions = {}) {
    this.options = {
      title: 'NEEX',
      showLogo: true,
      interactive: true,
      ...options
    };
    this.updateTermSize();
  }

  /**
   * Update terminal size
   */
  private updateTermSize(): void {
    this.termWidth = process.stdout.columns || 80;
    this.termHeight = process.stdout.rows || 24;
    this.leftPanelWidth = Math.min(30, Math.floor(this.termWidth * 0.25));
  }

  /**
   * Add a task
   */
  addTask(name: string, command: string): void {
    this.tasks.set(name, {
      name,
      command,
      status: 'pending',
      output: [],
    });
  }

  /**
   * Update task status
   */
  updateTask(name: string, update: Partial<TUITask>): void {
    const task = this.tasks.get(name);
    if (task) {
      Object.assign(task, update);
      if (this.running) this.render();
    }
  }

  /**
   * Append output to task
   */
  appendOutput(name: string, line: string): void {
    const task = this.tasks.get(name);
    if (task) {
      task.output.push(line);
      if (this.running) this.render();
    }
  }

  /**
   * Render the NEEX logo
   */
  private renderLogo(): string {
    return chalk.magenta.bold(' NEEX ');
  }

  /**
   * Render the header
   */
  private renderHeader(): string {
    const logo = this.renderLogo();
    const taskCount = this.tasks.size;
    const running = [...this.tasks.values()].filter(t => t.status === 'running').length;
    const done = [...this.tasks.values()].filter(t => t.status === 'success').length;
    
    const status = `${chalk.cyan(running)} running ${chalk.dim('|')} ${chalk.green(done)}/${taskCount} done`;
    const padding = ' '.repeat(Math.max(0, this.termWidth - logo.length - status.length - 10));
    
    return `${logo}${padding}${status}\n${chalk.dim('─'.repeat(this.termWidth))}\n`;
  }

  /**
   * Render task list (left panel)
   */
  private renderTaskList(): string[] {
    const lines: string[] = [];
    const taskArray = [...this.tasks.values()];
    
    lines.push(chalk.dim('Tasks (/ - Search)'));
    lines.push('');

    taskArray.forEach((task, index) => {
      const isSelected = index === this.selectedIndex;
      const prefix = isSelected ? chalk.magenta('» ') : '  ';
      
      let statusIcon = '';
      let nameColor = chalk.white;
      
      switch (task.status) {
        case 'pending':
          statusIcon = chalk.dim('○');
          nameColor = chalk.dim;
          break;
        case 'running':
          statusIcon = chalk.yellow('●');
          nameColor = chalk.yellow;
          break;
        case 'success':
          statusIcon = chalk.green('✓');
          nameColor = chalk.green;
          break;
        case 'error':
          statusIcon = chalk.red('✗');
          nameColor = chalk.red;
          break;
      }

      const displayName = task.name.length > this.leftPanelWidth - 6 
        ? task.name.substring(0, this.leftPanelWidth - 9) + '...'
        : task.name;

      const line = `${prefix}${statusIcon} ${nameColor(displayName)}`;
      lines.push(isSelected ? chalk.inverse(line.padEnd(this.leftPanelWidth - 1)) : line);
    });

    return lines;
  }

  /**
   * Render output panel (right side)
   */
  private renderOutputPanel(): string[] {
    const lines: string[] = [];
    const taskArray = [...this.tasks.values()];
    const selectedTask = taskArray[this.selectedIndex];

    if (!selectedTask) {
      lines.push(chalk.dim('No task selected'));
      return lines;
    }

    // Header with command
    const cacheInfo = selectedTask.status === 'running' 
      ? chalk.yellow('executing...') 
      : selectedTask.status === 'success'
        ? chalk.green(`completed in ${this.formatDuration(selectedTask.endTime! - selectedTask.startTime!)}`)
        : '';
    
    lines.push(`${chalk.dim('$')} ${chalk.bold(selectedTask.command)}`);
    lines.push(chalk.dim(cacheInfo));
    lines.push('');

    // Output lines
    const maxOutputLines = this.termHeight - 8;
    const outputStart = Math.max(0, selectedTask.output.length - maxOutputLines);
    
    for (let i = outputStart; i < selectedTask.output.length; i++) {
      lines.push(selectedTask.output[i]);
    }

    return lines;
  }

  /**
   * Render keyboard shortcuts (bottom bar)
   */
  private renderFooter(): string {
    const shortcuts = [
      { key: '↑ ↓', desc: 'Select' },
      { key: 'i', desc: 'Interact' },
      { key: 'u/d', desc: 'Scroll logs' },
      { key: 'U/D', desc: 'Page logs' },
      { key: 't/b', desc: 'Jump to top/bottom' },
    ];

    const shortcutStr = shortcuts
      .map(s => `${chalk.dim(s.key)} ${chalk.dim('-')} ${s.desc}`)
      .join('    ');

    return `${chalk.dim('─'.repeat(this.termWidth))}\n${shortcutStr}\n${chalk.dim('m - More binds')}`;
  }

  /**
   * Format duration
   */
  private formatDuration(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  /**
   * Full render
   */
  render(): void {
    this.updateTermSize();
    
    let output = '';
    output += ansi.moveToTop;
    output += ansi.clearScreen;
    
    // Header
    output += this.renderHeader();

    // Main content area
    const taskLines = this.renderTaskList();
    const outputLines = this.renderOutputPanel();
    const contentHeight = this.termHeight - 6;

    for (let row = 0; row < contentHeight; row++) {
      // Left panel
      const taskLine = taskLines[row] || '';
      output += taskLine.padEnd(this.leftPanelWidth);
      
      // Separator
      output += chalk.dim('│ ');
      
      // Right panel
      const outputLine = outputLines[row] || '';
      const rightWidth = this.termWidth - this.leftPanelWidth - 3;
      output += outputLine.substring(0, rightWidth);
      
      output += '\n';
    }

    // Footer
    output += this.renderFooter();

    process.stdout.write(output);
  }

  /**
   * Start TUI mode
   */
  start(): void {
    this.running = true;
    process.stdout.write(ansi.hideCursor);
    this.render();
  }

  /**
   * Stop TUI mode
   */
  stop(): void {
    this.running = false;
    process.stdout.write(ansi.showCursor);
    process.stdout.write(ansi.clearScreen);
    process.stdout.write(ansi.moveToTop);
  }

  /**
   * Select next task
   */
  selectNext(): void {
    if (this.selectedIndex < this.tasks.size - 1) {
      this.selectedIndex++;
      this.render();
    }
  }

  /**
   * Select previous task
   */
  selectPrev(): void {
    if (this.selectedIndex > 0) {
      this.selectedIndex--;
      this.render();
    }
  }
}

// ============================================================================
// Simple Logger (non-TUI mode)
// ============================================================================

export class SimpleLogger {
  private startTime: number = Date.now();

  /**
   * Print NEEX header
   */
  printHeader(taskCount: number, concurrency: number): void {
    console.log();
    console.log(chalk.magenta.bold(' NEEX ') + chalk.dim('Task Runner'));
    console.log(chalk.dim('─'.repeat(50)));
    console.log(`${chalk.cyan(figures.pointer)} Executing ${chalk.bold(taskCount)} tasks with max ${chalk.bold(concurrency)} parallel`);
    console.log();
  }

  /**
   * Print task start
   */
  printTaskStart(name: string, command: string): void {
    console.log(`${chalk.dim('[')}${chalk.cyan(name)}${chalk.dim(']')} ${chalk.yellow(figures.arrowRight)} ${chalk.dim('$')} ${command}`);
  }

  /**
   * Print task output
   */
  printTaskOutput(name: string, line: string): void {
    const prefix = `${chalk.dim('[')}${chalk.cyan(name)}${chalk.dim(']')}`;
    console.log(`${prefix} ${line}`);
  }

  /**
   * Print task success
   */
  printTaskSuccess(name: string, duration: number): void {
    const durationStr = duration < 1000 ? `${duration}ms` : `${(duration / 1000).toFixed(2)}s`;
    console.log(`${chalk.dim('[')}${chalk.green(name)}${chalk.dim(']')} ${chalk.green(figures.tick)} Completed in ${chalk.bold(durationStr)}`);
  }

  /**
   * Print task error
   */
  printTaskError(name: string, error: string): void {
    console.log(`${chalk.dim('[')}${chalk.red(name)}${chalk.dim(']')} ${chalk.red(figures.cross)} ${error}`);
  }

  /**
   * Print summary
   */
  printSummary(successful: number, failed: number): void {
    const totalTime = Date.now() - this.startTime;
    const timeStr = totalTime < 1000 ? `${totalTime}ms` : `${(totalTime / 1000).toFixed(2)}s`;
    
    console.log();
    console.log(chalk.dim('─'.repeat(50)));
    console.log(`${chalk.magenta(figures.pointer)} Task Summary:`);
    
    if (failed === 0) {
      console.log(`   ${chalk.green(figures.tick)} ${successful} successful`);
    } else {
      console.log(`   ${chalk.green(figures.tick)} ${successful} successful`);
      console.log(`   ${chalk.red(figures.cross)} ${failed} failed`);
    }
    
    console.log(`   ${chalk.blue(figures.info)} Total time: ${chalk.bold(timeStr)}`);
    console.log();
  }
}

export default { TerminalUI, SimpleLogger };
