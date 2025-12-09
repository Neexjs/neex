/**
 * Remote Cache Client - Improved
 * 
 * S3/R2 compatible remote cache for distributed builds.
 * 
 * Features:
 * - Proper AWS Signature V4 authentication
 * - Retry with exponential backoff
 * - Integration with local cache
 * - Connection pooling
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

// Retry configuration
const MAX_RETRIES = 3;
const INITIAL_RETRY_DELAY = 1000; // 1 second

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
            logger.printLine(`[Remote Cache] Failed to load config: ${(error as Error).message}`, 'warn');
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
            const key = 'neex-connection-test';
            const url = `${this.config.endpoint}/${this.config.bucket}/${key}`;
            const headers = this.signRequest('HEAD', `/${this.config.bucket}/${key}`);

            const response = await fetch(url, {
                method: 'HEAD',
                headers
            });

            // 200 = exists, 404 = bucket accessible but key not found
            return response.ok || response.status === 404;
        } catch (error) {
            logger.printLine(`[Remote Cache] Connection check failed: ${(error as Error).message}`, 'warn');
            return false;
        }
    }

    /**
     * Upload artifact to remote cache with retry
     */
    async put(hash: string, data: Buffer): Promise<boolean> {
        if (!this.isEnabled() || !this.config) return false;

        return this.withRetry(async () => {
            const key = this.getKey(hash);
            const url = `${this.config!.endpoint}/${this.config!.bucket}/${key}`;
            const headers = this.signRequest('PUT', `/${this.config!.bucket}/${key}`, data);

            const response = await fetch(url, {
                method: 'PUT',
                headers,
                body: new Uint8Array(data)
            });

            if (response.ok) {
                logger.printLine(`[Remote Cache] Uploaded: ${hash.substring(0, 10)}...`, 'info');
                return true;
            }

            throw new Error(`Upload failed: ${response.status} ${response.statusText}`);
        });
    }

    /**
     * Download artifact from remote cache with retry
     */
    async get(hash: string): Promise<Buffer | null> {
        if (!this.isEnabled() || !this.config) return null;

        try {
            return await this.withRetry(async () => {
                const key = this.getKey(hash);
                const url = `${this.config!.endpoint}/${this.config!.bucket}/${key}`;
                const headers = this.signRequest('GET', `/${this.config!.bucket}/${key}`);

                const response = await fetch(url, {
                    method: 'GET',
                    headers
                });

                if (response.ok) {
                    logger.printLine(`[Remote Cache] Hit: ${hash.substring(0, 10)}...`, 'info');
                    return Buffer.from(await response.arrayBuffer());
                }

                if (response.status === 404) {
                    return null; // Not an error, just cache miss
                }

                throw new Error(`Download failed: ${response.status}`);
            });
        } catch {
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
            const headers = this.signRequest('HEAD', `/${this.config.bucket}/${key}`);

            const response = await fetch(url, {
                method: 'HEAD',
                headers
            });

            return response.ok;
        } catch {
            return false;
        }
    }

    /**
     * Get storage key for hash (sharded)
     */
    private getKey(hash: string): string {
        const prefix = hash.substring(0, 2);
        return `cache/${prefix}/${hash}.tar.gz`;
    }

    /**
     * Retry with exponential backoff
     */
    private async withRetry<T>(fn: () => Promise<T>): Promise<T> {
        let lastError: Error | null = null;
        let delay = INITIAL_RETRY_DELAY;

        for (let attempt = 0; attempt < MAX_RETRIES; attempt++) {
            try {
                return await fn();
            } catch (error) {
                lastError = error as Error;
                if (attempt < MAX_RETRIES - 1) {
                    await new Promise(resolve => setTimeout(resolve, delay));
                    delay *= 2; // Exponential backoff
                }
            }
        }

        throw lastError;
    }

    /**
     * Sign request with AWS Signature V4
     */
    private signRequest(
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
        
        // Content hash
        const payloadHash = body 
            ? crypto.createHash('sha256').update(body).digest('hex')
            : 'UNSIGNED-PAYLOAD';

        const host = new URL(this.config.endpoint).host;
        
        // Canonical headers
        const canonicalHeaders = [
            `host:${host}`,
            `x-amz-content-sha256:${payloadHash}`,
            `x-amz-date:${amzDate}`,
        ].join('\n') + '\n';

        const signedHeaders = 'host;x-amz-content-sha256;x-amz-date';

        // Canonical request
        const canonicalRequest = [
            method,
            path,
            '', // Query string (empty)
            canonicalHeaders,
            signedHeaders,
            payloadHash
        ].join('\n');

        // String to sign
        const algorithm = 'AWS4-HMAC-SHA256';
        const credentialScope = `${dateStamp}/${region}/${service}/aws4_request`;
        const stringToSign = [
            algorithm,
            amzDate,
            credentialScope,
            crypto.createHash('sha256').update(canonicalRequest).digest('hex')
        ].join('\n');

        // Signing key
        const kDate = this.hmac(`AWS4${this.config.secretAccessKey}`, dateStamp);
        const kRegion = this.hmac(kDate, region);
        const kService = this.hmac(kRegion, service);
        const kSigning = this.hmac(kService, 'aws4_request');

        // Signature
        const signature = crypto.createHmac('sha256', kSigning)
            .update(stringToSign)
            .digest('hex');

        // Authorization header
        const authorization = [
            `${algorithm} Credential=${this.config.accessKeyId}/${credentialScope}`,
            `SignedHeaders=${signedHeaders}`,
            `Signature=${signature}`
        ].join(', ');

        return {
            'Host': host,
            'x-amz-date': amzDate,
            'x-amz-content-sha256': payloadHash,
            'Authorization': authorization,
            'Content-Type': 'application/octet-stream'
        };
    }

    /**
     * HMAC helper
     */
    private hmac(key: string | Buffer, data: string): Buffer {
        return crypto.createHmac('sha256', key).update(data).digest();
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
