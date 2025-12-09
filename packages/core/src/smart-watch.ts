/**
 * Smart Watch Mode - Affected-Only Rebuilds
 * 
 * Features:
 * - Native file watching (Bun's built-in watcher / FSEvents on macOS)
 * - Affected-only: rebuild only changed packages and their dependents
 * - Debouncing: prevent rapid rebuilds
 * - Streaming execution via TaskGraph
 * 
 * Better than Turbo/Nx:
 * - Smarter detection of what needs to rebuild
 * - Faster response to changes
 * - Lower memory footprint
 */

import * as path from 'path';
import * as fs from 'fs';
import chalk from 'chalk';
import figures from 'figures';
import { watch, type FSWatcher } from 'fs';
import { buildTaskGraph, PackageTask, TaskGraph } from './task-graph';

// ============================================================================
// Types
// ============================================================================

export interface WatchOptions {
  debounceMs?: number;          // Debounce delay (default: 300ms)
  maxConcurrency?: number;      // Max parallel tasks
  printOutput?: boolean;        // Stream output
  initialBuild?: boolean;       // Run initial build before watching
}

export interface PackageInfo {
  name: string;
  path: string;
  dependencies: string[];
  scripts: Record<string, string>;
}

// ============================================================================
// Smart Watcher
// ============================================================================

export class SmartWatcher {
  private rootDir: string;
  private taskName: string;
  private options: Required<WatchOptions>;
  private packages: Map<string, PackageInfo> = new Map();
  private watchers: FSWatcher[] = [];
  private debounceTimer: ReturnType<typeof setTimeout> | null = null;
  private pendingChanges: Set<string> = new Set();
  private isRebuilding = false;
  private packageManager: string = 'bun';
  private dependentsGraph: Map<string, Set<string>> = new Map();

  constructor(
    rootDir: string,
    taskName: string,
    options: WatchOptions = {}
  ) {
    this.rootDir = rootDir;
    this.taskName = taskName;
    this.options = {
      debounceMs: options.debounceMs ?? 300,
      maxConcurrency: options.maxConcurrency ?? 4,
      printOutput: options.printOutput ?? true,
      initialBuild: options.initialBuild ?? true,
    };
  }

  /**
   * Detect package manager
   */
  private async detectPackageManager(): Promise<string> {
    if (fs.existsSync(path.join(this.rootDir, 'pnpm-lock.yaml'))) return 'pnpm';
    if (fs.existsSync(path.join(this.rootDir, 'yarn.lock'))) return 'yarn';
    if (fs.existsSync(path.join(this.rootDir, 'bun.lockb'))) return 'bun';
    if (fs.existsSync(path.join(this.rootDir, 'bun.lock'))) return 'bun';
    return 'npm';
  }

  /**
   * Scan workspaces and build package map
   */
  private async scanWorkspaces(): Promise<void> {
    this.packages.clear();
    this.dependentsGraph.clear();
    
    // Read root package.json for workspaces
    const rootPkgPath = path.join(this.rootDir, 'package.json');
    if (!fs.existsSync(rootPkgPath)) {
      throw new Error('No package.json found in root directory');
    }

    const rootPkg = JSON.parse(fs.readFileSync(rootPkgPath, 'utf-8'));
    const workspacePatterns: string[] = rootPkg.workspaces || [];

    // Resolve workspace patterns
    for (const pattern of workspacePatterns) {
      const baseDir = pattern.replace('/*', '');
      const fullBaseDir = path.join(this.rootDir, baseDir);
      
      if (!fs.existsSync(fullBaseDir)) continue;

      const entries = fs.readdirSync(fullBaseDir, { withFileTypes: true });
      
      for (const entry of entries) {
        if (!entry.isDirectory()) continue;
        
        const pkgPath = path.join(fullBaseDir, entry.name, 'package.json');
        if (!fs.existsSync(pkgPath)) continue;

        try {
          const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));
          if (!pkg.name) continue;

          const pkgInfo: PackageInfo = {
            name: pkg.name,
            path: path.join(fullBaseDir, entry.name),
            dependencies: [
              ...Object.keys(pkg.dependencies || {}),
              ...Object.keys(pkg.devDependencies || {}),
            ],
            scripts: pkg.scripts || {},
          };

          this.packages.set(pkg.name, pkgInfo);
        } catch (e) {
          // Invalid package.json, skip
        }
      }
    }

    // Build dependents graph (reverse of dependencies)
    const internalPkgNames = new Set(this.packages.keys());
    
    for (const [name, pkg] of this.packages) {
      for (const dep of pkg.dependencies) {
        if (internalPkgNames.has(dep)) {
          if (!this.dependentsGraph.has(dep)) {
            this.dependentsGraph.set(dep, new Set());
          }
          this.dependentsGraph.get(dep)!.add(name);
        }
      }
    }
  }

  /**
   * Find which package a file belongs to
   */
  private findPackageForFile(filePath: string): string | null {
    const absPath = path.isAbsolute(filePath) 
      ? filePath 
      : path.join(this.rootDir, filePath);

    for (const [name, pkg] of this.packages) {
      if (absPath.startsWith(pkg.path)) {
        return name;
      }
    }
    return null;
  }

  /**
   * Get all affected packages (changed + dependents)
   */
  private getAffectedPackages(changedPackages: Set<string>): Set<string> {
    const affected = new Set<string>(changedPackages);
    const queue = [...changedPackages];

    while (queue.length > 0) {
      const pkg = queue.shift()!;
      const dependents = this.dependentsGraph.get(pkg);
      
      if (dependents) {
        for (const dep of dependents) {
          if (!affected.has(dep)) {
            affected.add(dep);
            queue.push(dep);
          }
        }
      }
    }

    return affected;
  }

  /**
   * Rebuild affected packages
   */
  private async rebuildAffected(changedPackages: Set<string>): Promise<void> {
    if (this.isRebuilding) return;
    this.isRebuilding = true;

    try {
      const affected = this.getAffectedPackages(changedPackages);
      
      // Filter to packages that have the task
      const packagesToRebuild = [...affected].filter(name => {
        const pkg = this.packages.get(name);
        return pkg && pkg.scripts[this.taskName];
      });

      if (packagesToRebuild.length === 0) {
        this.isRebuilding = false;
        return;
      }

      console.log(
        chalk.yellow(`\n${figures.arrowRight} Changes detected in: `) +
        chalk.cyan([...changedPackages].join(', '))
      );
      console.log(
        chalk.yellow(`${figures.arrowRight} Rebuilding: `) +
        chalk.cyan(packagesToRebuild.join(', '))
      );

      // Build tasks for TaskGraph
      const internalPkgNames = new Set(this.packages.keys());
      const tasks: PackageTask[] = packagesToRebuild.map(name => {
        const pkg = this.packages.get(name)!;
        const internalDeps = pkg.dependencies.filter(d => internalPkgNames.has(d));
        
        return {
          packageName: name,
          taskName: this.taskName,
          command: `${this.packageManager} run ${this.taskName}`,
          cwd: pkg.path,
          internalDeps,
        };
      });

      // Execute with TaskGraph (streaming)
      const graph = buildTaskGraph(tasks, true, {
        maxConcurrency: this.options.maxConcurrency,
        stopOnError: false, // Don't stop on error in watch mode
        printOutput: this.options.printOutput,
        color: true,
      });

      await graph.execute();

    } finally {
      this.isRebuilding = false;
    }
  }

  /**
   * Handle file change event
   */
  private handleChange(filePath: string): void {
    // Ignore non-source files
    if (
      filePath.includes('node_modules') ||
      filePath.includes('.git') ||
      filePath.includes('dist') ||
      filePath.includes('.next') ||
      filePath.includes('.neex')
    ) {
      return;
    }

    const pkg = this.findPackageForFile(filePath);
    if (!pkg) return;

    this.pendingChanges.add(pkg);

    // Debounce
    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer);
    }

    this.debounceTimer = setTimeout(async () => {
      const changes = new Set(this.pendingChanges);
      this.pendingChanges.clear();
      
      if (changes.size > 0) {
        await this.rebuildAffected(changes);
      }
    }, this.options.debounceMs);
  }

  /**
   * Setup file watchers for all packages
   */
  private setupWatchers(): void {
    for (const [name, pkg] of this.packages) {
      // Watch src directory if exists, otherwise watch package root
      const srcDir = path.join(pkg.path, 'src');
      const watchDir = fs.existsSync(srcDir) ? srcDir : pkg.path;

      try {
        const watcher = watch(
          watchDir,
          { recursive: true },
          (eventType, filename) => {
            if (filename) {
              this.handleChange(path.join(watchDir, filename));
            }
          }
        );

        this.watchers.push(watcher);
      } catch (err) {
        console.warn(
          chalk.yellow(`${figures.warning} Could not watch ${name}: ${(err as Error).message}`)
        );
      }
    }
  }

  /**
   * Start watching
   */
  async start(): Promise<void> {
    console.log(chalk.blue(`\n${figures.pointer} Smart Watch Mode\n`));
    
    // Detect package manager
    this.packageManager = await this.detectPackageManager();
    console.log(chalk.gray(`Using package manager: ${this.packageManager}`));

    // Scan workspaces
    console.log(chalk.gray('Scanning workspaces...'));
    await this.scanWorkspaces();
    console.log(chalk.gray(`Found ${this.packages.size} packages`));

    // Initial build if enabled
    if (this.options.initialBuild) {
      console.log(chalk.yellow(`\n${figures.arrowRight} Running initial ${this.taskName}...`));
      
      const allPackages = new Set(this.packages.keys());
      await this.rebuildAffected(allPackages);
    }

    // Setup watchers
    console.log(chalk.gray('\nSetting up file watchers...'));
    this.setupWatchers();
    
    console.log(
      chalk.green(`\n${figures.tick} Watching for changes...`) +
      chalk.gray(` (Press Ctrl+C to stop)\n`)
    );

    // Handle shutdown
    const cleanup = () => {
      console.log(chalk.yellow(`\n${figures.warning} Stopping watch mode...`));
      this.stop();
      process.exit(0);
    };

    process.on('SIGINT', cleanup);
    process.on('SIGTERM', cleanup);
  }

  /**
   * Stop watching
   */
  stop(): void {
    for (const watcher of this.watchers) {
      watcher.close();
    }
    this.watchers = [];

    if (this.debounceTimer) {
      clearTimeout(this.debounceTimer);
      this.debounceTimer = null;
    }
  }
}

// ============================================================================
// Helper function
// ============================================================================

export async function watchTask(
  rootDir: string,
  taskName: string,
  options?: WatchOptions
): Promise<void> {
  const watcher = new SmartWatcher(rootDir, taskName, options);
  await watcher.start();
  
  // Keep process alive
  await new Promise(() => {});
}

export default SmartWatcher;
