/**
 * Project Graph - Incremental with SQLite Persistence
 * 
 * Features:
 * - SQLite storage for fast graph persistence
 * - Incremental updates (only changed packages)
 * - Lazy loading of dependencies (on-demand)
 * - Better than Nx: no full graph rebuild every time
 * 
 * Uses Bun's built-in SQLite for maximum performance
 */

import { Database } from 'bun:sqlite';
import * as fs from 'fs';
import * as path from 'path';
import chalk from 'chalk';
import figures from 'figures';

// ============================================================================
// Types
// ============================================================================

export interface PackageNode {
  name: string;
  path: string;
  version: string;
  dependencies: string[];      // Internal deps
  devDependencies: string[];   // Internal dev deps
  scripts: Record<string, string>;
  lastModified: number;        // mtime of package.json
  hash: string;                // Content hash for change detection
}

export interface GraphStats {
  totalPackages: number;
  cachedPackages: number;
  updatedPackages: number;
  loadTimeMs: number;
}

// ============================================================================
// Project Graph with SQLite
// ============================================================================

export class ProjectGraph {
  private db: Database;
  private rootDir: string;
  private packages: Map<string, PackageNode> = new Map();
  private dependentsMap: Map<string, Set<string>> = new Map();
  private isLoaded = false;
  private stats: GraphStats = { totalPackages: 0, cachedPackages: 0, updatedPackages: 0, loadTimeMs: 0 };

  constructor(rootDir: string) {
    this.rootDir = rootDir;
    const cacheDir = path.join(rootDir, '.neex', 'cache');
    
    // Ensure cache directory exists
    if (!fs.existsSync(cacheDir)) {
      fs.mkdirSync(cacheDir, { recursive: true });
    }

    // Initialize SQLite database
    const dbPath = path.join(cacheDir, 'project-graph.db');
    this.db = new Database(dbPath);
    this.initSchema();
  }

  /**
   * Initialize SQLite schema
   */
  private initSchema(): void {
    this.db.run(`
      CREATE TABLE IF NOT EXISTS packages (
        name TEXT PRIMARY KEY,
        path TEXT NOT NULL,
        version TEXT,
        dependencies TEXT,
        devDependencies TEXT,
        scripts TEXT,
        lastModified INTEGER,
        hash TEXT
      )
    `);

    this.db.run(`
      CREATE INDEX IF NOT EXISTS idx_packages_hash ON packages(hash)
    `);

    this.db.run(`
      CREATE TABLE IF NOT EXISTS meta (
        key TEXT PRIMARY KEY,
        value TEXT
      )
    `);
  }

  /**
   * Compute simple hash of package.json content
   */
  private computeHash(content: string): string {
    let hash = 0;
    for (let i = 0; i < content.length; i++) {
      const char = content.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash = hash & hash; // Convert to 32-bit integer
    }
    return hash.toString(16);
  }

  /**
   * Get cached package from SQLite
   */
  private getCached(name: string): PackageNode | null {
    const stmt = this.db.prepare('SELECT * FROM packages WHERE name = ?');
    const row = stmt.get(name) as any;
    
    if (!row) return null;

    return {
      name: row.name,
      path: row.path,
      version: row.version || '0.0.0',
      dependencies: JSON.parse(row.dependencies || '[]'),
      devDependencies: JSON.parse(row.devDependencies || '[]'),
      scripts: JSON.parse(row.scripts || '{}'),
      lastModified: row.lastModified,
      hash: row.hash,
    };
  }

  /**
   * Save package to SQLite
   */
  private savePackage(pkg: PackageNode): void {
    const stmt = this.db.prepare(`
      INSERT OR REPLACE INTO packages 
      (name, path, version, dependencies, devDependencies, scripts, lastModified, hash)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `);
    
    stmt.run(
      pkg.name,
      pkg.path,
      pkg.version,
      JSON.stringify(pkg.dependencies),
      JSON.stringify(pkg.devDependencies),
      JSON.stringify(pkg.scripts),
      pkg.lastModified,
      pkg.hash
    );
  }

  /**
   * Check if package.json has changed
   */
  private hasChanged(pkgJsonPath: string, cached: PackageNode | null): boolean {
    if (!cached) return true;
    
    try {
      const stat = fs.statSync(pkgJsonPath);
      return stat.mtimeMs > cached.lastModified;
    } catch {
      return true;
    }
  }

  /**
   * Parse package.json and extract relevant info
   */
  private parsePackageJson(pkgJsonPath: string, internalPkgs: Set<string>): PackageNode | null {
    try {
      const content = fs.readFileSync(pkgJsonPath, 'utf-8');
      const pkg = JSON.parse(content);
      const stat = fs.statSync(pkgJsonPath);

      if (!pkg.name) return null;

      // Filter to only internal dependencies
      const deps = Object.keys(pkg.dependencies || {}).filter(d => internalPkgs.has(d));
      const devDeps = Object.keys(pkg.devDependencies || {}).filter(d => internalPkgs.has(d));

      return {
        name: pkg.name,
        path: path.dirname(pkgJsonPath),
        version: pkg.version || '0.0.0',
        dependencies: deps,
        devDependencies: devDeps,
        scripts: pkg.scripts || {},
        lastModified: stat.mtimeMs,
        hash: this.computeHash(content),
      };
    } catch {
      return null;
    }
  }

  /**
   * Scan workspace patterns and find all packages
   */
  private findPackagePaths(): string[] {
    const rootPkgPath = path.join(this.rootDir, 'package.json');
    if (!fs.existsSync(rootPkgPath)) return [];

    const rootPkg = JSON.parse(fs.readFileSync(rootPkgPath, 'utf-8'));
    const patterns: string[] = rootPkg.workspaces || [];
    const result: string[] = [];

    for (const pattern of patterns) {
      const baseDir = pattern.replace('/*', '');
      const fullDir = path.join(this.rootDir, baseDir);
      
      if (!fs.existsSync(fullDir)) continue;

      const entries = fs.readdirSync(fullDir, { withFileTypes: true });
      for (const entry of entries) {
        if (!entry.isDirectory()) continue;
        
        const pkgJsonPath = path.join(fullDir, entry.name, 'package.json');
        if (fs.existsSync(pkgJsonPath)) {
          result.push(pkgJsonPath);
        }
      }
    }

    return result;
  }

  /**
   * Build dependents map (reverse of dependencies)
   */
  private buildDependentsMap(): void {
    this.dependentsMap.clear();

    for (const [name, pkg] of this.packages) {
      const allDeps = [...pkg.dependencies, ...pkg.devDependencies];
      
      for (const dep of allDeps) {
        if (!this.dependentsMap.has(dep)) {
          this.dependentsMap.set(dep, new Set());
        }
        this.dependentsMap.get(dep)!.add(name);
      }
    }
  }

  /**
   * Load graph incrementally
   * - Uses cached data when package.json hasn't changed
   * - Only parses changed packages
   */
  async load(): Promise<GraphStats> {
    const startTime = Date.now();
    this.packages.clear();
    this.stats = { totalPackages: 0, cachedPackages: 0, updatedPackages: 0, loadTimeMs: 0 };

    // Find all package paths
    const pkgPaths = this.findPackagePaths();
    this.stats.totalPackages = pkgPaths.length;

    // First pass: get all package names for internal dependency detection
    const internalPkgNames = new Set<string>();
    for (const pkgPath of pkgPaths) {
      try {
        const content = fs.readFileSync(pkgPath, 'utf-8');
        const pkg = JSON.parse(content);
        if (pkg.name) internalPkgNames.add(pkg.name);
      } catch { /* skip */ }
    }

    // Second pass: load packages incrementally
    for (const pkgJsonPath of pkgPaths) {
      try {
        const content = fs.readFileSync(pkgJsonPath, 'utf-8');
        const tempPkg = JSON.parse(content);
        if (!tempPkg.name) continue;

        const cached = this.getCached(tempPkg.name);
        
        if (cached && !this.hasChanged(pkgJsonPath, cached)) {
          // Use cached data
          this.packages.set(cached.name, cached);
          this.stats.cachedPackages++;
        } else {
          // Parse and cache
          const node = this.parsePackageJson(pkgJsonPath, internalPkgNames);
          if (node) {
            this.packages.set(node.name, node);
            this.savePackage(node);
            this.stats.updatedPackages++;
          }
        }
      } catch { /* skip */ }
    }

    // Build reverse dependency map
    this.buildDependentsMap();

    this.stats.loadTimeMs = Date.now() - startTime;
    this.isLoaded = true;

    return this.stats;
  }

  /**
   * Get package by name (lazy loading)
   */
  getPackage(name: string): PackageNode | undefined {
    // First check in-memory
    if (this.packages.has(name)) {
      return this.packages.get(name);
    }

    // Try to load from SQLite cache
    const cached = this.getCached(name);
    if (cached) {
      this.packages.set(name, cached);
      return cached;
    }

    return undefined;
  }

  /**
   * Get all packages
   */
  getAllPackages(): Map<string, PackageNode> {
    return this.packages;
  }

  /**
   * Get packages that depend on a given package
   */
  getDependents(name: string): string[] {
    return [...(this.dependentsMap.get(name) || [])];
  }

  /**
   * Get all affected packages (changed + dependents transitively)
   */
  getAffected(changedPackages: string[]): string[] {
    const affected = new Set<string>(changedPackages);
    const queue = [...changedPackages];

    while (queue.length > 0) {
      const pkg = queue.shift()!;
      const dependents = this.getDependents(pkg);
      
      for (const dep of dependents) {
        if (!affected.has(dep)) {
          affected.add(dep);
          queue.push(dep);
        }
      }
    }

    return [...affected];
  }

  /**
   * Get topological order for task execution
   */
  getTopologicalOrder(): string[] {
    const visited = new Set<string>();
    const result: string[] = [];

    const visit = (name: string, ancestors: Set<string>) => {
      if (visited.has(name)) return;
      if (ancestors.has(name)) return; // Circular dependency

      ancestors.add(name);

      const pkg = this.packages.get(name);
      if (pkg) {
        for (const dep of [...pkg.dependencies, ...pkg.devDependencies]) {
          if (this.packages.has(dep)) {
            visit(dep, ancestors);
          }
        }
      }

      ancestors.delete(name);
      visited.add(name);
      result.push(name);
    };

    for (const name of this.packages.keys()) {
      visit(name, new Set());
    }

    return result;
  }

  /**
   * Get statistics
   */
  getStats(): GraphStats {
    return this.stats;
  }

  /**
   * Print graph summary
   */
  printSummary(): void {
    console.log(chalk.blue(`\n${figures.pointer} Project Graph Summary`));
    console.log(`   ${chalk.gray('Total packages:')} ${this.stats.totalPackages}`);
    console.log(`   ${chalk.green('Cached (fast):')} ${this.stats.cachedPackages}`);
    console.log(`   ${chalk.yellow('Updated:')} ${this.stats.updatedPackages}`);
    console.log(`   ${chalk.blue('Load time:')} ${this.stats.loadTimeMs}ms\n`);
  }

  /**
   * Clear all cached data
   */
  clearCache(): void {
    this.db.run('DELETE FROM packages');
    this.db.run('DELETE FROM meta');
    this.packages.clear();
    this.dependentsMap.clear();
  }

  /**
   * Close database connection
   */
  close(): void {
    this.db.close();
  }
}

export default ProjectGraph;
