/**
 * @deprecated This file is no longer used. 
 * WASM loading is now handled directly in native.ts
 * 
 * This file is kept for backward compatibility but does nothing.
 */

export function getLibraryPath(): string {
    throw new Error(
        'FFI loader is deprecated. Neex now uses WebAssembly.\n' +
        'Please update your code to use the new WASM-based API.'
    );
}

export default { getLibraryPath };
