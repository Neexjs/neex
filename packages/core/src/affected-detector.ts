/**
 * Affected Detection
 * 
 * Smart detection of which packages need to be rebuilt.
 * Uses git diff to find changed files and dependency graph
 * to determine affected packages.
 * 
 * Features:
 * - Git-based change detection
 * - Transitive dependency resolution
 * - Incremental hash comparison
 */

import * as fs from 'fs';
import * as path from 'path';
import { spawn } from 'child_process';
import { getTracker } from './incremental-tracker';
import logger from './logger';

export interface AffectedPackage {
    name: string;
    path: string;
    reason: 'changed' | 'dependency' | 'all';
    changedFiles: string[];
}

export interface DependencyGraph {
    nodes: Map<string, { name: string; path: string; dependencies: string[] }>;
}

export class AffectedDetector {
    private rootDir: string;
    private graph: DependencyGraph = { nodes: new Map() };

    constructor(rootDir: string) {
        this.rootDir = rootDir;
    }

    /**
     * Execute git command and return output
     */
    private async gitCommand(args: string[]): Promise<string> {
        return new Promise((resolve, reject) => {
            const proc = spawn('git', args, {
                cwd: this.rootDir,
                stdio: ['pipe', 'pipe', 'pipe'],
            });

            let stdout = '';
            let stderr = '';

            proc.stdout.on('data', data => {
                stdout += data.toString();
            });

            proc.stderr.on('data', data => {
                stderr += data.toString();
            });

            proc.on('close', code => {
                if (code === 0) {
                    resolve(stdout.trim());
                } else {
                    reject(new Error(`Git command failed: ${stderr}`));
                }
            });

            proc.on('error', reject);
        });
    }

    /**
     * Get list of files changed since a ref (commit, branch, tag)
     */
    async getChangedFilesSince(ref: string = 'HEAD~1'): Promise<string[]> {
        try {
            const output = await this.gitCommand(['diff', '--name-only', ref]);
            return output.split('\n').filter(Boolean);
        } catch {
            // If git fails (not a repo or no commits), return empty
            return [];
        }
    }

    /**
     * Get list of uncommitted changes
     */
    async getUncommittedChanges(): Promise<string[]> {
        try {
            // Staged + unstaged changes
            const staged = await this.gitCommand(['diff', '--name-only', '--cached']);
            const unstaged = await this.gitCommand(['diff', '--name-only']);
            const untracked = await this.gitCommand(['ls-files', '--others', '--exclude-standard']);
            
            const allChanges = new Set([
                ...staged.split('\n').filter(Boolean),
                ...unstaged.split('\n').filter(Boolean),
                ...untracked.split('\n').filter(Boolean),
            ]);

            return Array.from(allChanges);
        } catch {
            return [];
        }
    }

    /**
     * Build dependency graph from workspace packages
     */
    async buildDependencyGraph(packages: Map<string, { name: string; path: string; dependencies: string[] }>): Promise<void> {
        this.graph.nodes = packages;
    }

    /**
     * Find which package a file belongs to
     */
    findPackageForFile(filePath: string): string | null {
        const absolutePath = path.isAbsolute(filePath) 
            ? filePath 
            : path.join(this.rootDir, filePath);

        for (const [name, pkg] of this.graph.nodes) {
            if (absolutePath.startsWith(pkg.path)) {
                return name;
            }
        }

        return null;
    }

    /**
     * Get transitive dependents (packages that depend on this one)
     */
    getTransitiveDependents(packageName: string, visited: Set<string> = new Set()): string[] {
        if (visited.has(packageName)) return [];
        visited.add(packageName);

        const dependents: string[] = [];

        for (const [name, pkg] of this.graph.nodes) {
            if (pkg.dependencies.includes(packageName)) {
                dependents.push(name);
                // Recursively find dependents of dependents
                dependents.push(...this.getTransitiveDependents(name, visited));
            }
        }

        return [...new Set(dependents)];
    }

    /**
     * Detect affected packages based on changed files
     */
    async detectAffected(
        changedFiles: string[],
        includeTransitive: boolean = true
    ): Promise<AffectedPackage[]> {
        const affectedMap = new Map<string, AffectedPackage>();

        // Find directly changed packages
        for (const file of changedFiles) {
            const packageName = this.findPackageForFile(file);
            
            if (packageName) {
                const pkg = this.graph.nodes.get(packageName);
                if (pkg) {
                    if (!affectedMap.has(packageName)) {
                        affectedMap.set(packageName, {
                            name: packageName,
                            path: pkg.path,
                            reason: 'changed',
                            changedFiles: [],
                        });
                    }
                    affectedMap.get(packageName)!.changedFiles.push(file);
                }
            }
        }

        // Find transitively affected packages
        if (includeTransitive) {
            const directlyChanged = Array.from(affectedMap.keys());
            
            for (const changedPkg of directlyChanged) {
                const dependents = this.getTransitiveDependents(changedPkg);
                
                for (const dependent of dependents) {
                    if (!affectedMap.has(dependent)) {
                        const pkg = this.graph.nodes.get(dependent);
                        if (pkg) {
                            affectedMap.set(dependent, {
                                name: dependent,
                                path: pkg.path,
                                reason: 'dependency',
                                changedFiles: [],
                            });
                        }
                    }
                }
            }
        }

        return Array.from(affectedMap.values());
    }

    /**
     * Detect affected packages since last commit
     */
    async detectAffectedSinceCommit(ref: string = 'HEAD~1'): Promise<AffectedPackage[]> {
        const changedFiles = await this.getChangedFilesSince(ref);
        return this.detectAffected(changedFiles);
    }

    /**
     * Detect affected packages from uncommitted changes
     */
    async detectAffectedUncommitted(): Promise<AffectedPackage[]> {
        const changedFiles = await this.getUncommittedChanges();
        return this.detectAffected(changedFiles);
    }

    /**
     * Get topological order for affected packages
     * (dependencies first, then dependents)
     */
    getTopologicalOrder(affected: AffectedPackage[]): AffectedPackage[] {
        const affectedNames = new Set(affected.map(p => p.name));
        const visited = new Set<string>();
        const order: string[] = [];

        const visit = (name: string) => {
            if (visited.has(name)) return;
            visited.add(name);

            const pkg = this.graph.nodes.get(name);
            if (!pkg) return;

            // Visit dependencies first (if they're also affected)
            for (const dep of pkg.dependencies) {
                if (affectedNames.has(dep)) {
                    visit(dep);
                }
            }

            order.push(name);
        };

        for (const pkg of affected) {
            visit(pkg.name);
        }

        // Return in topological order
        return order.map(name => affected.find(p => p.name === name)!).filter(Boolean);
    }

    /**
     * Log affected packages summary
     */
    logAffected(affected: AffectedPackage[]): void {
        if (affected.length === 0) {
            logger.printLine('[Affected] No packages affected by changes', 'info');
            return;
        }

        logger.printLine(`[Affected] ${affected.length} package(s) affected:`, 'info');
        
        const changed = affected.filter(p => p.reason === 'changed');
        const dependent = affected.filter(p => p.reason === 'dependency');

        if (changed.length > 0) {
            logger.printLine(`  Directly changed: ${changed.map(p => p.name).join(', ')}`, 'info');
        }
        if (dependent.length > 0) {
            logger.printLine(`  Transitively affected: ${dependent.map(p => p.name).join(', ')}`, 'info');
        }
    }
}

export default AffectedDetector;
