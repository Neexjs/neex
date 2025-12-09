/**
 * Task Graph Engine - Streaming Execution
 * 
 * Features:
 * - Event-driven task execution
 * - Parallel boundaries (max concurrency)
 * - Streaming: start tasks as soon as dependencies complete
 * - Better than Turbo/Nx: smarter graph traversal
 * - Beautiful NEEX-branded terminal output
 */

import { spawn, type Subprocess } from 'bun';
import chalk from 'chalk';
import figures from 'figures';
import { SimpleLogger } from './terminal-ui';

// ============================================================================
// Types
// ============================================================================

export type TaskStatus = 'pending' | 'ready' | 'running' | 'success' | 'failed' | 'skipped';

export interface TaskNode {
  id: string;                    // Unique task ID: "pkg:task" e.g. "web:build"
  packageName: string;           // Package name
  taskName: string;              // Task name (build, dev, etc.)
  command: string;               // Full command to execute
  cwd: string;                   // Working directory
  status: TaskStatus;
  dependencies: string[];        // Task IDs this depends on
  dependents: string[];          // Task IDs that depend on this
  startTime?: number;
  endTime?: number;
  exitCode?: number;
  error?: Error;
}

export interface TaskGraphOptions {
  maxConcurrency?: number;       // Max parallel tasks (default: CPU cores)
  stopOnError?: boolean;         // Stop all on first error
  printOutput?: boolean;         // Stream output to console
  color?: boolean;               // Colored output
}

export interface TaskResult {
  taskId: string;
  success: boolean;
  duration: number;
  exitCode: number | null;
}

// ============================================================================
// Task Graph
// ============================================================================

export class TaskGraph {
  private nodes: Map<string, TaskNode> = new Map();
  private options: Required<TaskGraphOptions>;
  private runningCount = 0;
  private completedCount = 0;
  private failedCount = 0;
  private activeProcesses: Map<string, Subprocess> = new Map();
  
  // Event callbacks
  private onTaskStart?: (task: TaskNode) => void;
  private onTaskComplete?: (task: TaskNode, result: TaskResult) => void;
  private onAllComplete?: (results: TaskResult[]) => void;

  constructor(options: TaskGraphOptions = {}) {
    const cpuCount = navigator?.hardwareConcurrency || 4;
    this.options = {
      maxConcurrency: options.maxConcurrency ?? cpuCount,
      stopOnError: options.stopOnError ?? true,
      printOutput: options.printOutput ?? true,
      color: options.color ?? true,
    };
  }

  // ---------------------------------------------------------------------------
  // Graph Building
  // ---------------------------------------------------------------------------

  addTask(node: Omit<TaskNode, 'status' | 'dependents'>): void {
    const taskNode: TaskNode = {
      ...node,
      status: 'pending',
      dependents: [],
    };
    this.nodes.set(node.id, taskNode);
  }

  /**
   * Build dependency graph edges (dependents)
   * Must be called after all tasks are added
   */
  buildGraph(): void {
    // Build reverse edges (dependents)
    for (const [id, node] of this.nodes) {
      for (const depId of node.dependencies) {
        const depNode = this.nodes.get(depId);
        if (depNode) {
          depNode.dependents.push(id);
        }
      }
    }

    // Mark tasks with no dependencies as ready
    for (const [id, node] of this.nodes) {
      if (node.dependencies.length === 0) {
        node.status = 'ready';
      }
    }
  }

  // ---------------------------------------------------------------------------
  // Execution Engine
  // ---------------------------------------------------------------------------

  /**
   * Get all tasks that are ready to run
   */
  private getReadyTasks(): TaskNode[] {
    const ready: TaskNode[] = [];
    for (const node of this.nodes.values()) {
      if (node.status === 'ready') {
        ready.push(node);
      }
    }
    return ready;
  }

  /**
   * Check if all dependencies of a task are complete
   */
  private areDependenciesComplete(node: TaskNode): boolean {
    for (const depId of node.dependencies) {
      const dep = this.nodes.get(depId);
      if (!dep || dep.status !== 'success') {
        return false;
      }
    }
    return true;
  }

  /**
   * Check if any dependency failed
   */
  private hasFailedDependency(node: TaskNode): boolean {
    for (const depId of node.dependencies) {
      const dep = this.nodes.get(depId);
      if (dep && (dep.status === 'failed' || dep.status === 'skipped')) {
        return true;
      }
    }
    return false;
  }

  /**
   * Update task statuses after a task completes
   * This is the core of streaming execution
   */
  private updateReadyTasks(): void {
    for (const node of this.nodes.values()) {
      if (node.status === 'pending') {
        if (this.hasFailedDependency(node)) {
          node.status = 'skipped';
          this.completedCount++;
        } else if (this.areDependenciesComplete(node)) {
          node.status = 'ready';
        }
      }
    }
  }

  /**
   * Execute a single task
   */
  private async executeTask(task: TaskNode): Promise<TaskResult> {
    task.status = 'running';
    task.startTime = Date.now();
    this.runningCount++;

    const prefix = this.options.color
      ? chalk.cyan(`[${task.packageName}]`)
      : `[${task.packageName}]`;

    if (this.options.printOutput) {
      console.log(`${prefix} ${chalk.yellow(figures.play)} Starting ${task.taskName}...`);
    }

    try {
      const proc = spawn({
        cmd: ['sh', '-c', task.command],
        cwd: task.cwd,
        stdout: 'pipe',
        stderr: 'pipe',
        env: { ...process.env, FORCE_COLOR: '1' },
      });

      this.activeProcesses.set(task.id, proc);

      // Stream output
      if (this.options.printOutput) {
        this.streamOutput(proc, task.packageName, 'stdout');
        this.streamOutput(proc, task.packageName, 'stderr');
      }

      const exitCode = await proc.exited;
      this.activeProcesses.delete(task.id);

      task.endTime = Date.now();
      task.exitCode = exitCode;
      const duration = task.endTime - task.startTime;

      if (exitCode === 0) {
        task.status = 'success';
        if (this.options.printOutput) {
          console.log(
            `${prefix} ${chalk.green(figures.tick)} Completed in ${this.formatDuration(duration)}`
          );
        }
      } else {
        task.status = 'failed';
        this.failedCount++;
        if (this.options.printOutput) {
          console.log(
            `${prefix} ${chalk.red(figures.cross)} Failed with exit code ${exitCode}`
          );
        }
      }

      this.runningCount--;
      this.completedCount++;

      return {
        taskId: task.id,
        success: exitCode === 0,
        duration,
        exitCode,
      };
    } catch (error) {
      task.status = 'failed';
      task.error = error as Error;
      task.endTime = Date.now();
      this.runningCount--;
      this.completedCount++;
      this.failedCount++;

      return {
        taskId: task.id,
        success: false,
        duration: task.endTime - (task.startTime || task.endTime),
        exitCode: -1,
      };
    }
  }

  /**
   * Stream process output
   */
  private async streamOutput(
    proc: Subprocess,
    packageName: string,
    type: 'stdout' | 'stderr'
  ): Promise<void> {
    const stream = type === 'stdout' ? proc.stdout : proc.stderr;
    if (!stream || typeof stream === 'number') return;

    const prefix = this.options.color
      ? chalk.gray(`${packageName}:`)
      : `${packageName}:`;

    // Bun ReadableStream
    const reader = (stream as ReadableStream<Uint8Array>).getReader();
    const decoder = new TextDecoder();

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        
        const text = decoder.decode(value);
        const lines = text.split('\n');
        for (const line of lines) {
          if (line.trim()) {
            console.log(`${prefix} ${line}`);
          }
        }
      }
    } catch (e) {
      // Stream closed, ignore
    }
  }

  /**
   * Main execution loop - Streaming Execution
   * This is what makes neex faster than Turbo/Nx
   */
  async execute(): Promise<TaskResult[]> {
    const results: TaskResult[] = [];
    const totalTasks = this.nodes.size;

    // NEEX branded header
    console.log();
    console.log(chalk.magenta.bold(' NEEX ') + chalk.dim('Task Runner'));
    console.log(chalk.dim('─'.repeat(50)));
    console.log(`${chalk.cyan(figures.pointer)} Executing ${chalk.bold(totalTasks)} tasks with max ${chalk.bold(this.options.maxConcurrency)} parallel`);
    console.log();

    const startTime = Date.now();

    // Promise-based streaming execution
    const running: Map<string, Promise<TaskResult>> = new Map();

    const scheduleReady = () => {
      // Get ready tasks and start them up to maxConcurrency
      const ready = this.getReadyTasks();
      
      for (const task of ready) {
        if (this.runningCount >= this.options.maxConcurrency) break;
        if (running.has(task.id)) continue;

        // Start task immediately
        const promise = this.executeTask(task).then((result) => {
          results.push(result);
          running.delete(task.id);
          
          // STREAMING: Update dependents immediately
          this.updateReadyTasks();
          
          // Schedule new ready tasks
          scheduleReady();
          
          return result;
        });

        running.set(task.id, promise);
      }
    };

    // Initial scheduling
    scheduleReady();

    // Wait for all tasks to complete
    while (this.completedCount < totalTasks) {
      if (running.size === 0 && this.completedCount < totalTasks) {
        // Deadlock or all remaining tasks are blocked
        break;
      }

      // Wait for at least one task to complete
      if (running.size > 0) {
        await Promise.race(running.values());
      }

      // Check for stop on error
      if (this.options.stopOnError && this.failedCount > 0) {
        console.log(chalk.red(`\n${figures.cross} Stopping due to task failure\n`));
        this.cleanup();
        break;
      }
    }

    const totalDuration = Date.now() - startTime;

    // Summary
    const successCount = results.filter(r => r.success).length;
    const failCount = results.filter(r => !r.success).length;
    const skippedCount = Array.from(this.nodes.values()).filter(n => n.status === 'skipped').length;

    // NEEX branded summary
    console.log();
    console.log(chalk.dim('─'.repeat(50)));
    console.log(`${chalk.magenta(figures.pointer)} Task Summary:`);
    console.log(`   ${chalk.green(figures.tick)} ${successCount} successful`);
    if (failCount > 0) console.log(`   ${chalk.red(figures.cross)} ${failCount} failed`);
    if (skippedCount > 0) console.log(`   ${chalk.yellow(figures.warning)} ${skippedCount} skipped`);
    console.log(`   ${chalk.blue(figures.info)} Total time: ${chalk.bold(this.formatDuration(totalDuration))}`);
    console.log();

    return results;
  }

  /**
   * Cleanup running processes
   */
  cleanup(): void {
    for (const [id, proc] of this.activeProcesses) {
      try {
        proc.kill();
      } catch {
        // Ignore
      }
    }
    this.activeProcesses.clear();
  }

  /**
   * Format duration in human-readable format
   */
  private formatDuration(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60000) return `${(ms / 1000).toFixed(2)}s`;
    const mins = Math.floor(ms / 60000);
    const secs = ((ms % 60000) / 1000).toFixed(1);
    return `${mins}m ${secs}s`;
  }

  // ---------------------------------------------------------------------------
  // Statistics
  // ---------------------------------------------------------------------------

  getStats(): { total: number; completed: number; failed: number; running: number } {
    return {
      total: this.nodes.size,
      completed: this.completedCount,
      failed: this.failedCount,
      running: this.runningCount,
    };
  }

  getNodes(): Map<string, TaskNode> {
    return this.nodes;
  }
}

// ============================================================================
// Helper: Build Task Graph from Package Dependencies
// ============================================================================

export interface PackageTask {
  packageName: string;
  taskName: string;
  command: string;
  cwd: string;
  internalDeps: string[];  // Other package names this depends on
}

/**
 * Build a task graph from package tasks with dependency awareness
 * 
 * @param tasks - List of package tasks
 * @param dependsOnUpstream - If true, task depends on same task in dependencies
 * @param options - Graph options
 */
export function buildTaskGraph(
  tasks: PackageTask[],
  dependsOnUpstream: boolean,
  options?: TaskGraphOptions
): TaskGraph {
  const graph = new TaskGraph(options);
  
  // Create task map for quick lookup
  const taskMap = new Map<string, PackageTask>();
  for (const task of tasks) {
    taskMap.set(task.packageName, task);
  }

  // Add tasks with dependencies
  for (const task of tasks) {
    const taskId = `${task.packageName}:${task.taskName}`;
    
    // Calculate dependencies
    const dependencies: string[] = [];
    
    if (dependsOnUpstream) {
      // Task depends on same task in internal dependencies
      for (const depPkg of task.internalDeps) {
        if (taskMap.has(depPkg)) {
          dependencies.push(`${depPkg}:${task.taskName}`);
        }
      }
    }

    graph.addTask({
      id: taskId,
      packageName: task.packageName,
      taskName: task.taskName,
      command: task.command,
      cwd: task.cwd,
      dependencies,
    });
  }

  graph.buildGraph();
  return graph;
}

export default TaskGraph;
