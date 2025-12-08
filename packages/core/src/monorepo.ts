import path from 'path';
import { Runner } from './runner';
import logger from './logger';
import { Native } from './native';
import { Semaphore } from './semaphore';
import { ZeroConfig } from './zero-config';
import { AffectedDetector, AffectedPackage } from './affected-detector';
import { TaskGraph, buildTaskGraph, PackageTask } from './task-graph';

export interface NeexConfig {
  pipeline?: Record<string, TaskConfig>;
}

export interface TaskConfig {
  dependsOn?: string[];
  outputs?: string[];
  inputs?: string[];
  cache?: boolean;
  persistent?: boolean;
}

export interface PackageInfo {
  name: string;
  path: string;
  dependencies: string[]; // Internal dependencies names
  scripts: Record<string, string>;
}

export class MonorepoManager {
  private rootDir: string;
  private config: NeexConfig | null = null;
  private packages: Map<string, PackageInfo> = new Map();
  private runner: Runner;

  constructor(rootDir: string, runner: Runner) {
    this.rootDir = rootDir;
    this.runner = runner;
  }

  /**
   * Load configuration with Zero-Config fallback
   * First tries neex.json, then auto-generates from package.json
   */
  async loadConfig(): Promise<void> {
    try {
      this.config = await ZeroConfig.loadWithFallback(this.rootDir);
    } catch (error) {
      logger.printLine(`Failed to load config: ${(error as Error).message}`, 'warn');
      this.config = {};
    }
  }

  async scanWorkspaces(): Promise<void> {
    try {
      // Use Native Engine for instant discovery (Zig + Bun)
      const relativePackageJsonPaths = Native.scan(this.rootDir);
      
      for (const relPath of relativePackageJsonPaths) {
          // Prevent infinite recursion: Do not treat the root package as a workspace
          if (relPath === 'package.json') continue;

          const packageJsonPath = path.join(this.rootDir, relPath);
          const pkgDir = path.dirname(packageJsonPath);

          // FIX: Explicitly ignore root directory to prevent infinite recursion
          if (pkgDir === this.rootDir || relPath === 'package.json' || relPath === './package.json') {
             continue;
          }

          try {
            const pkg = await Bun.file(packageJsonPath).json();
            
            // Basic validation
            if (!pkg.name) continue;

            this.packages.set(pkg.name, {
                name: pkg.name,
                path: pkgDir,
                dependencies: [
                    ...Object.keys(pkg.dependencies || {}),
                    ...Object.keys(pkg.devDependencies || {})
                ],
                scripts: pkg.scripts || {}
            });
        } catch(e) { /* ignore invalid json */ }
      }

    } catch (error) {
      logger.printLine(`Error scanning workspaces: ${(error as Error).message}`, 'error');
    }
  }

  getPackage(name: string): PackageInfo | undefined {
    return this.packages.get(name);
  }

  getPackages(): Map<string, PackageInfo> {
    return this.packages;
  }


  private getTopologicalOrder(): string[] {
    const visited = new Set<string>();
    const order: string[] = [];
    const internalPkgNames = new Set(this.packages.keys());

    const visit = (pkgName: string, ancestors: Set<string>) => {
      if (ancestors.has(pkgName)) {
        logger.printLine(`Circular dependency detected involving ${pkgName}`, 'warn');
        return;
      }
      if (visited.has(pkgName)) return;

      const pkg = this.packages.get(pkgName);
      if (!pkg) return;

      ancestors.add(pkgName);
      
      for (const dep of pkg.dependencies) {
        if (internalPkgNames.has(dep)) {
            visit(dep, ancestors);
        }
      }

      visited.add(pkgName);
      ancestors.delete(pkgName);
      order.push(pkgName);
    };

    for (const pkgName of this.packages.keys()) {
      visit(pkgName, new Set());
    }

    return order;
  }


  private async getPackageManager(): Promise<string> {
    const fs = require('fs');
    
    if (fs.existsSync(path.join(this.rootDir, 'pnpm-lock.yaml'))) return 'pnpm';
    if (fs.existsSync(path.join(this.rootDir, 'yarn.lock'))) return 'yarn';
    if (fs.existsSync(path.join(this.rootDir, 'bun.lockb'))) return 'bun';
    if (fs.existsSync(path.join(this.rootDir, 'bun.lock'))) return 'bun';
    if (fs.existsSync(path.join(this.rootDir, 'package-lock.json'))) return 'npm';
    
    return 'npm';
  }

  async runTask(taskName: string): Promise<void> {
    await this.loadConfig();
    await this.scanWorkspaces();

    const taskConfig = this.config?.pipeline?.[taskName];
    const isPersistent = taskConfig?.persistent ?? false;
    const dependsOn = taskConfig?.dependsOn || [];
    
    // Check for ^ (upstream dependencies) - this triggers graph-based execution
    const dependsOnUpstream = dependsOn.some(d => d.startsWith('^'));
    
    const pkgNames = Array.from(this.packages.keys());
    
    // Filter packages that actually have the script
    const packagesWithScript = pkgNames.filter(name => !!this.packages.get(name)?.scripts[taskName]);
    
    if (packagesWithScript.length === 0) {
      logger.printLine(`No packages found with script: ${taskName}`, 'warn');
      return;
    }

    const pm = await this.getPackageManager();
    
    // Get internal package names for dependency resolution
    const internalPkgNames = new Set(this.packages.keys());

    // Build task list for TaskGraph
    const tasks: PackageTask[] = packagesWithScript.map(name => {
      const pkg = this.packages.get(name)!;
      
      // Find internal dependencies (only packages in this monorepo)
      const internalDeps = pkg.dependencies.filter(dep => internalPkgNames.has(dep));
      
      return {
        packageName: name,
        taskName,
        command: `${pm} run ${taskName}`,
        cwd: pkg.path,
        internalDeps,
      };
    });

    // =========================================================================
    // STREAMING TASK GRAPH EXECUTION
    // This is what makes neex faster than Turbo/Nx:
    // - Tasks start as soon as their dependencies complete
    // - No waiting for entire "phase" to finish
    // - Maximum parallelism with dependency awareness
    // =========================================================================

    const graph = buildTaskGraph(tasks, dependsOnUpstream, {
      maxConcurrency: isPersistent ? tasks.length : undefined, // No limit for persistent
      stopOnError: !isPersistent,  // Don't stop dev servers on error
      printOutput: true,
      color: true,
    });

    // Execute with streaming
    const results = await graph.execute();
    
    // Check for failures
    const failed = results.filter(r => !r.success);
    if (failed.length > 0 && !isPersistent) {
      throw new Error(`${failed.length} task(s) failed`);
    }
  }

  /**
   * Run task only on affected packages
   * Uses git to detect changes and dependency graph for transitives
   */
  async runAffected(taskName: string, base: string = 'HEAD~1'): Promise<void> {
    await this.loadConfig();
    await this.scanWorkspaces();

    // Build detector with our packages
    const detector = new AffectedDetector(this.rootDir);
    await detector.buildDependencyGraph(this.packages);

    // Detect affected packages
    const affected = await detector.detectAffectedSinceCommit(base);

    if (affected.length === 0) {
      logger.printLine(`[Affected] No packages affected since ${base}`, 'info');
      return;
    }

    detector.logAffected(affected);

    // Get topological order
    const orderedAffected = detector.getTopologicalOrder(affected);
    
    // Filter to packages that have the script
    const packagesWithScript = orderedAffected.filter(
      pkg => !!this.packages.get(pkg.name)?.scripts[taskName]
    );

    if (packagesWithScript.length === 0) {
      logger.printLine(`[Affected] No affected packages have script: ${taskName}`, 'warn');
      return;
    }

    const pm = await this.getPackageManager();
    const taskConfig = this.config?.pipeline?.[taskName];
    const isPersistent = taskConfig?.persistent ?? false;

    logger.printLine(
      `[Affected] Running ${taskName} on ${packagesWithScript.length} package(s)...`,
      'info'
    );

    const commands = packagesWithScript.map(pkg => {
      return `cd ${pkg.path} && ${pm} run ${taskName}`;
    });

    if (isPersistent) {
      await this.runner.runParallel(commands);
    } else {
      // Run in topological order for non-persistent tasks
      await this.runner.runSequential(commands);
    }
  }
}
