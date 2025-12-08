/**
 * Compression Utilities
 * Uses Bun's native gzip/gunzip for fast compression
 * 
 * Note: Zstd would be better (3x faster), but Bun doesn't have native support yet.
 * Gzip is still good (~2.5x compression) and universally compatible.
 */

/**
 * Compress data using gzip
 * @param data - Raw data to compress
 * @returns Compressed data
 */
export function compress(data: Uint8Array): Uint8Array {
    // Convert to Buffer for Bun compatibility
    const buffer = Buffer.from(data);
    return new Uint8Array(Bun.gzipSync(buffer, { level: 6 }));
}

/**
 * Decompress gzip data
 * @param data - Compressed data
 * @returns Decompressed data
 */
export function decompress(data: Uint8Array): Uint8Array {
    const buffer = Buffer.from(data);
    return new Uint8Array(Bun.gunzipSync(buffer));
}

/**
 * Compress a string
 */
export function compressString(str: string): Uint8Array {
    const encoder = new TextEncoder();
    return compress(encoder.encode(str));
}

/**
 * Decompress to string
 */
export function decompressString(data: Uint8Array): string {
    const decoder = new TextDecoder();
    return decoder.decode(decompress(data));
}

/**
 * Get compression ratio
 */
export function getCompressionRatio(original: number, compressed: number): number {
    if (compressed === 0) return 0;
    return original / compressed;
}

export default {
    compress,
    decompress,
    compressString,
    decompressString,
    getCompressionRatio
};
