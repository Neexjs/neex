import path from 'path';
import { Runner } from './runner';
import logger from './logger';
import { Native } from './native';
import { Semaphore } from './semaphore';
import { ZeroConfig } from './zero-config';

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
    if (await Bun.file(path.join(this.rootDir, 'pnpm-lock.yaml')).exists()) return 'pnpm';
    if (await Bun.file(path.join(this.rootDir, 'yarn.lock')).exists()) return 'yarn';
    if (await Bun.file(path.join(this.rootDir, 'bun.lockb')).exists()) return 'bun';
    if (await Bun.file(path.join(this.rootDir, 'bun.lock')).exists()) return 'bun';
    return 'npm';
  }

  async runTask(taskName: string): Promise<void> {
    await this.loadConfig();
    await this.scanWorkspaces();

    const taskConfig = this.config?.pipeline?.[taskName];
    const isPersistent = taskConfig?.persistent ?? false;
    const dependsOn = taskConfig?.dependsOn || [];
    
    // Check if task depends on itself in dependencies (topological execution)
    // dependsOn: ["^build"] means run build on dependencies first.
    
    const topoOrder = this.getTopologicalOrder();
    const pkgNames = Array.from(this.packages.keys());
    
    // Filter packages that actually have the script
    const packagesWithScript = pkgNames.filter(name => !!this.packages.get(name)?.scripts[taskName]);
    
    if (packagesWithScript.length === 0) {
        logger.printLine(`No packages found with script: ${taskName}`, 'warn');
        return;
    }

    const pm = await this.getPackageManager();

    if (isPersistent) {
        // Run parallel
        logger.printLine(`Running ${taskName} in parallel for: ${packagesWithScript.join(', ')}`, 'info');
        const commands = packagesWithScript.map(name => {
            const pkg = this.packages.get(name)!;
            // Use package manager run to ensure PATH is correct
            return `cd ${pkg.path} && ${pm} run ${taskName}`; 
        });
        await this.runner.runParallel(commands);
    } else {
        // Check for ^ (upstream dependencies)
        const dependsOnUpstream = dependsOn.some(d => d.startsWith('^'));
        
        if (dependsOnUpstream) {
            // Run in topological order
             logger.printLine(`Running ${taskName} in topological order...`, 'info');
             const orderedPackages = topoOrder.filter(name => packagesWithScript.includes(name));
             
             const commands = orderedPackages.map(name => {
                 const pkg = this.packages.get(name)!;
                 return `cd ${pkg.path} && ${pm} run ${taskName}`;
             });
             
             await this.runner.runSequential(commands);
        } else {
            // Run in parallel
            logger.printLine(`Running ${taskName} in parallel (no upstream dependency enforcement)...`, 'info');
            const commands = packagesWithScript.map(name => {
                const pkg = this.packages.get(name)!;
                return `cd ${pkg.path} && ${pm} run ${taskName}`;
            });
            await this.runner.runParallel(commands);
        }
    }
  }
}
