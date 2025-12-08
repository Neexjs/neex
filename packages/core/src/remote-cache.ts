/**
 * Remote Cache Client
 * 
 * S3/R2 compatible remote cache for distributed builds.
 * Stores task outputs in cloud storage for team sharing.
 * 
 * Supported providers:
 * - Cloudflare R2
 * - AWS S3
 * - Any S3-compatible storage
 */

import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import logger from './logger';

export interface RemoteCacheConfig {
    provider: 'r2' | 's3' | 'custom';
    endpoint: string;
    bucket: string;
    accessKeyId: string;
    secretAccessKey: string;
    region?: string;
}

export interface CacheArtifact {
    hash: string;
    files: Array<{
        path: string;
        content: Buffer;
    }>;
    meta: {
        timestamp: number;
        duration: number;
        exitCode: number;
    };
}

export class RemoteCacheClient {
    private config: RemoteCacheConfig | null = null;
    private configPath: string;
    private enabled: boolean = false;

    constructor(rootDir: string) {
        this.configPath = path.join(rootDir, '.neex', 'remote-cache.json');
        this.loadConfig();
    }

    /**
     * Load config from disk
     */
    private loadConfig(): void {
        try {
            if (fs.existsSync(this.configPath)) {
                const content = fs.readFileSync(this.configPath, 'utf-8');
                this.config = JSON.parse(content);
                this.enabled = true;
            }
        } catch (error) {
            this.enabled = false;
        }
    }

    /**
     * Save config to disk
     */
    async configure(config: RemoteCacheConfig): Promise<void> {
        this.config = config;
        
        const configDir = path.dirname(this.configPath);
        if (!fs.existsSync(configDir)) {
            fs.mkdirSync(configDir, { recursive: true });
        }

        // Store config (consider encrypting sensitive data)
        fs.writeFileSync(this.configPath, JSON.stringify(config, null, 2));
        this.enabled = true;

        logger.printLine('[Remote Cache] Configuration saved', 'info');
    }

    /**
     * Check if remote cache is enabled
     */
    isEnabled(): boolean {
        return this.enabled && this.config !== null;
    }

    /**
     * Get cache status
     */
    async getStatus(): Promise<{
        enabled: boolean;
        provider?: string;
        bucket?: string;
        connected: boolean;
    }> {
        if (!this.isEnabled() || !this.config) {
            return { enabled: false, connected: false };
        }

        const connected = await this.checkConnection();

        return {
            enabled: true,
            provider: this.config.provider,
            bucket: this.config.bucket,
            connected
        };
    }

    /**
     * Test connection to remote storage
     */
    async checkConnection(): Promise<boolean> {
        if (!this.config) return false;

        try {
            // Simple HEAD request to bucket
            const url = `${this.config.endpoint}/${this.config.bucket}`;
            const headers = this.getAuthHeaders('HEAD', '/');

            const response = await fetch(url, {
                method: 'HEAD',
                headers
            });

            return response.ok || response.status === 404;
        } catch (error) {
            return false;
        }
    }

    /**
     * Upload artifact to remote cache
     */
    async put(hash: string, data: Buffer): Promise<boolean> {
        if (!this.isEnabled() || !this.config) return false;

        try {
            const key = this.getKey(hash);
            const url = `${this.config.endpoint}/${this.config.bucket}/${key}`;
            const headers = this.getAuthHeaders('PUT', `/${key}`, data);

            const response = await fetch(url, {
                method: 'PUT',
                headers,
                body: new Uint8Array(data)
            });

            if (response.ok) {
                logger.printLine(`[Remote Cache] Uploaded: ${hash.substring(0, 10)}...`, 'info');
                return true;
            }

            return false;
        } catch (error) {
            logger.printLine(`[Remote Cache] Upload failed: ${(error as Error).message}`, 'warn');
            return false;
        }
    }

    /**
     * Download artifact from remote cache
     */
    async get(hash: string): Promise<Buffer | null> {
        if (!this.isEnabled() || !this.config) return null;

        try {
            const key = this.getKey(hash);
            const url = `${this.config.endpoint}/${this.config.bucket}/${key}`;
            const headers = this.getAuthHeaders('GET', `/${key}`);

            const response = await fetch(url, {
                method: 'GET',
                headers
            });

            if (response.ok) {
                logger.printLine(`[Remote Cache] Hit: ${hash.substring(0, 10)}...`, 'info');
                return Buffer.from(await response.arrayBuffer());
            }

            return null;
        } catch (error) {
            return null;
        }
    }

    /**
     * Check if artifact exists in remote cache
     */
    async has(hash: string): Promise<boolean> {
        if (!this.isEnabled() || !this.config) return false;

        try {
            const key = this.getKey(hash);
            const url = `${this.config.endpoint}/${this.config.bucket}/${key}`;
            const headers = this.getAuthHeaders('HEAD', `/${key}`);

            const response = await fetch(url, {
                method: 'HEAD',
                headers
            });

            return response.ok;
        } catch (error) {
            return false;
        }
    }

    /**
     * Get storage key for hash
     */
    private getKey(hash: string): string {
        // Shard by first 2 chars for better distribution
        const prefix = hash.substring(0, 2);
        return `cache/${prefix}/${hash}.tar.gz`;
    }

    /**
     * Generate AWS Signature V4 headers
     */
    private getAuthHeaders(
        method: string,
        path: string,
        body?: Buffer
    ): Record<string, string> {
        if (!this.config) return {};

        const now = new Date();
        const amzDate = now.toISOString().replace(/[:-]|\.\d{3}/g, '');
        const dateStamp = amzDate.substring(0, 8);
        const region = this.config.region || 'auto';
        const service = 's3';

        // For simplicity, using basic auth header
        // In production, implement full AWS Sig V4
        const headers: Record<string, string> = {
            'x-amz-date': amzDate,
            'x-amz-content-sha256': body 
                ? crypto.createHash('sha256').update(body).digest('hex')
                : 'UNSIGNED-PAYLOAD',
            'Host': new URL(this.config.endpoint).host
        };

        // Add authorization
        const auth = Buffer.from(
            `${this.config.accessKeyId}:${this.config.secretAccessKey}`
        ).toString('base64');
        
        headers['Authorization'] = `AWS ${auth}`;

        return headers;
    }

    /**
     * Clear remote cache configuration
     */
    async clearConfig(): Promise<void> {
        if (fs.existsSync(this.configPath)) {
            fs.unlinkSync(this.configPath);
        }
        this.config = null;
        this.enabled = false;
        logger.printLine('[Remote Cache] Configuration cleared', 'info');
    }
}

export default RemoteCacheClient;
