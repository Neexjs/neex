/**
 * Zero-Config Detection
 * 
 * Automatically detects project configuration from package.json
 * when neex.json is not present. Makes Neex work out-of-the-box
 * for simple projects.
 * 
 * Features:
 * - Auto-detect scripts (build, dev, test, lint)
 * - Smart defaults for cache/outputs
 * - Framework detection (Next.js, Express, etc.)
 */

import * as fs from 'fs';
import * as path from 'path';
import { NeexConfig, TaskConfig } from './types';
import logger from './logger';

export interface PackageJson {
    name?: string;
    version?: string;
    scripts?: Record<string, string>;
    dependencies?: Record<string, string>;
    devDependencies?: Record<string, string>;
    workspaces?: string[] | { packages: string[] };
}

export interface DetectedFramework {
    name: string;
    type: 'frontend' | 'backend' | 'fullstack' | 'library';
    outputDir: string;
}

/**
 * Known frameworks and their configurations
 */
const FRAMEWORK_PATTERNS: Array<{
    dependency: string;
    framework: DetectedFramework;
}> = [
    { dependency: 'next', framework: { name: 'Next.js', type: 'frontend', outputDir: '.next' } },
    { dependency: 'nuxt', framework: { name: 'Nuxt', type: 'frontend', outputDir: '.nuxt' } },
    { dependency: 'vite', framework: { name: 'Vite', type: 'frontend', outputDir: 'dist' } },
    { dependency: 'react-scripts', framework: { name: 'Create React App', type: 'frontend', outputDir: 'build' } },
    { dependency: 'express', framework: { name: 'Express', type: 'backend', outputDir: 'dist' } },
    { dependency: 'fastify', framework: { name: 'Fastify', type: 'backend', outputDir: 'dist' } },
    { dependency: 'hono', framework: { name: 'Hono', type: 'backend', outputDir: 'dist' } },
    { dependency: 'elysia', framework: { name: 'Elysia', type: 'backend', outputDir: 'dist' } },
    { dependency: 'nestjs', framework: { name: 'NestJS', type: 'backend', outputDir: 'dist' } },
];

/**
 * Default task configurations
 */
const DEFAULT_TASK_CONFIG: Record<string, Partial<TaskConfig>> = {
    build: {
        outputs: ['dist', 'build', '.next', '.nuxt'],
        cache: true,
        dependsOn: ['^build'], // Build dependencies first
    },
    dev: {
        cache: false,
        persistent: true,
    },
    test: {
        outputs: ['coverage'],
        cache: true,
    },
    lint: {
        cache: true,
    },
    typecheck: {
        cache: true,
    },
    'type-check': {
        cache: true,
    },
};

export class ZeroConfig {
    private rootDir: string;
    private packageJson: PackageJson | null = null;

    constructor(rootDir: string) {
        this.rootDir = rootDir;
    }

    /**
     * Load package.json
     */
    private async loadPackageJson(): Promise<PackageJson | null> {
        if (this.packageJson) return this.packageJson;

        const pkgPath = path.join(this.rootDir, 'package.json');
        
        try {
            if (fs.existsSync(pkgPath)) {
                const content = fs.readFileSync(pkgPath, 'utf-8');
                this.packageJson = JSON.parse(content);
                return this.packageJson;
            }
        } catch (e) {
            logger.printLine(`Failed to parse package.json: ${(e as Error).message}`, 'warn');
        }

        return null;
    }

    /**
     * Detect framework from dependencies
     */
    async detectFramework(): Promise<DetectedFramework | null> {
        const pkg = await this.loadPackageJson();
        if (!pkg) return null;

        const allDeps = {
            ...pkg.dependencies,
            ...pkg.devDependencies,
        };

        for (const pattern of FRAMEWORK_PATTERNS) {
            if (allDeps[pattern.dependency]) {
                return pattern.framework;
            }
        }

        return null;
    }

    /**
     * Detect if this is a monorepo
     */
    async isMonorepo(): Promise<boolean> {
        const pkg = await this.loadPackageJson();
        if (!pkg) return false;

        // Check for workspaces field
        if (pkg.workspaces) return true;

        // Check for common monorepo config files
        const monorepoFiles = [
            'pnpm-workspace.yaml',
            'lerna.json',
            'nx.json',
            'rush.json',
        ];

        for (const file of monorepoFiles) {
            if (fs.existsSync(path.join(this.rootDir, file))) {
                return true;
            }
        }

        return false;
    }

    /**
     * Get available scripts from package.json
     */
    async getAvailableScripts(): Promise<string[]> {
        const pkg = await this.loadPackageJson();
        if (!pkg?.scripts) return [];

        return Object.keys(pkg.scripts);
    }

    /**
     * Generate smart defaults for a task
     */
    private generateTaskConfig(scriptName: string, scriptCommand: string): TaskConfig {
        // Start with defaults if available
        const defaults = DEFAULT_TASK_CONFIG[scriptName] || {};
        
        const config: TaskConfig = {
            cache: defaults.cache ?? false,
            persistent: defaults.persistent ?? false,
        };

        // Detect outputs from script command
        if (scriptCommand.includes('tsc') || scriptCommand.includes('typescript')) {
            config.outputs = ['dist'];
            config.cache = true;
        }
        
        if (scriptCommand.includes('next build')) {
            config.outputs = ['.next'];
            config.cache = true;
        }

        if (scriptCommand.includes('vite build')) {
            config.outputs = ['dist'];
            config.cache = true;
        }

        // Detect if it's a dev/watch command
        if (scriptCommand.includes('--watch') || 
            scriptCommand.includes('-w') || 
            scriptCommand.includes('dev') ||
            scriptCommand.includes('start')) {
            config.persistent = true;
            config.cache = false;
        }

        // Apply defaults
        if (defaults.outputs && !config.outputs) {
            config.outputs = defaults.outputs;
        }
        if (defaults.dependsOn && !config.dependsOn) {
            config.dependsOn = defaults.dependsOn;
        }

        return config;
    }

    /**
     * Generate a complete neex.json configuration from package.json
     */
    async generateConfig(): Promise<NeexConfig> {
        const pkg = await this.loadPackageJson();
        if (!pkg?.scripts) {
            return {};
        }

        const pipeline: Record<string, TaskConfig> = {};

        for (const [scriptName, scriptCommand] of Object.entries(pkg.scripts)) {
            // Skip internal scripts
            if (scriptName.startsWith('pre') || scriptName.startsWith('post')) {
                continue;
            }

            pipeline[scriptName] = this.generateTaskConfig(scriptName, scriptCommand);
        }

        const framework = await this.detectFramework();
        
        logger.printLine(
            `[Zero-Config] Detected ${Object.keys(pipeline).length} scripts` +
            (framework ? `, framework: ${framework.name}` : ''),
            'info'
        );

        return {
            pipeline,
            performance: {
                hashingStrategy: 'auto',
            },
        };
    }

    /**
     * Load config with zero-config fallback
     * First tries neex.json, then auto-generates from package.json
     */
    static async loadWithFallback(rootDir: string): Promise<NeexConfig> {
        const neexConfigPath = path.join(rootDir, 'neex.json');

        // Try neex.json first
        if (fs.existsSync(neexConfigPath)) {
            try {
                const content = fs.readFileSync(neexConfigPath, 'utf-8');
                return JSON.parse(content);
            } catch (e) {
                logger.printLine(`Failed to parse neex.json: ${(e as Error).message}`, 'warn');
            }
        }

        // Fallback: auto-generate from package.json
        const zeroConfig = new ZeroConfig(rootDir);
        return zeroConfig.generateConfig();
    }
}

export default ZeroConfig;
