export declare function isAbortError(error: unknown): boolean;
export declare function sleep(ms: number, signal?: AbortSignal): Promise<void>;
interface FetchWithTimeoutOptions extends RequestInit {
    timeoutMs: number;
    signal?: AbortSignal;
}
export declare function fetchWithTimeout(url: URL, options: FetchWithTimeoutOptions): Promise<Response>;
export declare function fetchJson(url: URL, options: FetchWithTimeoutOptions): Promise<unknown>;
export declare function fetchOk(url: URL, options: FetchWithTimeoutOptions): Promise<void>;
export {};
